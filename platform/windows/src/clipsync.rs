//! Clipboard sync service: Windows clipboard ⇄ core `Hub` ⇄ iPad, on its own thread, separate from
//! the input engine so nothing here can ever stall keyboard/mouse. Logs kinds and sizes only.

use std::{sync::{Arc, Mutex}, thread, time::{Duration, SystemTime, UNIX_EPOCH}};

use bridge_core::{cliphistory::{self, History}, clipboard::{Content, Endpoint, Hub, MAX_ITEM_BYTES}};
use serde::Serialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{clipboard, diag, ipad::{self, IpadLink, IpadStatus, IpadWriter}};

pub enum ClipCommand {
    SetEnabled(bool),
    PairIpad,
    /// Puts a files pairing code on the iPad's clipboard (outside the sync), for Bridge there to paste.
    InviteToIpad(String),
    /// The paired phones that get Windows/iPad text (ids), replacing the previous list.
    Phones(Vec<String>),
    /// Text a phone sent (its clipboard tile).
    FromPhone { id: String, text: String },
    Stop,
}

/// Delivers text to a phone (the transfer service's `send_clip`). Text only: phones get no images.
pub type ToPhone = Box<dyn Fn(String, String) + Send>;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ClipStatus {
    pub enabled: bool,
    pub ipad: Option<IpadStatus>,
    pub synced: u64,
    pub skipped_too_large: u64,
    pub notice: Option<String>,
    /// Goes up whenever history stored something, so the hub knows to reload its list.
    pub history_rev: u64,
}

pub struct ClipHandle { tx: UnboundedSender<ClipCommand>, thread: Option<thread::JoinHandle<()>> }

impl ClipHandle {
    pub fn send(&self, c: ClipCommand) { let _ = self.tx.send(c); }
}

impl Drop for ClipHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(ClipCommand::Stop);
        if let Some(t) = self.thread.take() { let _ = t.join(); }
    }
}

const RECONNECT: Duration = Duration::from_secs(2);
/// Generous: a slow handshake (iPad half-asleep) must end in a retry, not hang reconnecting for good.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// 5 s plus 1 s per MiB: big screenshots need encoding and a slow link, small text must fail fast.
fn write_timeout(size: usize) -> Duration { Duration::from_secs(5 + (size / (1024 * 1024)) as u64) }

pub fn start(enabled: bool, history: Arc<Mutex<History>>, to_phone: ToPhone, on_status: impl Fn(ClipStatus) + Send + 'static) -> ClipHandle {
    let (tx, rx) = unbounded_channel();
    let thread = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        // Single-threaded on purpose: idevice futures don't prove Send, and nothing here needs more threads.
        tokio::task::LocalSet::new().block_on(&rt, run(enabled, history, rx, to_phone, on_status));
        // Don't let a pending mDNS lookup (up to 10 s, blocking pool) hold up Quit.
        rt.shutdown_background();
    });
    ClipHandle { tx, thread: Some(thread) }
}

/// Results of background work. `session` tags iPad events so a dead session's late errors are ignored.
enum Event {
    Connected(Result<Box<IpadLink>, String>),
    FromIpad { session: u64, change: Result<Content, String> },
    Paired(Result<(), String>),
    /// History stored an item (see `record`).
    Recorded,
}

async fn run(enabled: bool, history: Arc<Mutex<History>>, mut commands: UnboundedReceiver<ClipCommand>, to_phone: ToPhone, on_status: impl Fn(ClipStatus)) {
    let mut hub = Hub::new(vec![Endpoint::Windows, Endpoint::Ipad]);
    hub.set_paused(!enabled);
    let mut status = ClipStatus { enabled, ipad: Some(IpadStatus::Connecting), ..Default::default() };
    let mut reported = None;

    // Windows changes: the watcher's std channel, bridged into tokio.
    let (win_std_tx, win_std_rx) = std::sync::mpsc::channel();
    let mut watcher = clipboard::watch(win_std_tx);
    let (win_tx, mut from_windows) = unbounded_channel();
    thread::spawn(move || while let Ok(c) = win_std_rx.recv() { if win_tx.send(c).is_err() { break } });

    let (ev_tx, mut events) = unbounded_channel();
    let mut writer: Option<IpadWriter> = None;
    // Aborted with every session change: it holds a tunnel handle, and a tunnel lives until its last handle goes.
    let mut reader_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut session = 0u64;
    let mut connecting = false;
    let mut retry = tokio::time::Instant::now();

    loop {
        if writer.is_none() && !connecting && tokio::time::Instant::now() >= retry {
            connecting = true;
            let tx = ev_tx.clone();
            tokio::task::spawn_local(async move {
                let r = match tokio::time::timeout(CONNECT_TIMEOUT, IpadLink::connect(&ipad::ipad_dir())).await {
                    Ok(r) => r.map(Box::new),
                    Err(_) => Err("iPad didn't answer in time".into()),
                };
                let _ = tx.send(Event::Connected(r));
            });
        }
        if reported.as_ref() != Some(&status) { reported = Some(status.clone()); on_status(status.clone()); }

        let mut writes: Vec<(Endpoint, Content)> = Vec::new();
        tokio::select! {
            c = commands.recv() => match c {
                None | Some(ClipCommand::Stop) => break,
                Some(ClipCommand::SetEnabled(on)) => { hub.set_paused(!on); status.enabled = on; }
                Some(ClipCommand::InviteToIpad(code)) => {
                    let c = Content::Text(code);
                    status.notice = Some(match writer.as_mut() {
                        None => "The iPad's clipboard is not connected yet. Use the QR code instead.".into(),
                        Some(w) => {
                            hub.written_directly(Endpoint::Ipad, &c);
                            match tokio::time::timeout(write_timeout(c.size()), w.write(&c)).await {
                                Ok(Ok(())) => "The code is on the iPad's clipboard. In Bridge there, tap Pair, then Paste code.".into(),
                                _ => "Could not put the code on the iPad. Use the QR code instead.".into(),
                            }
                        }
                    });
                }
                Some(ClipCommand::PairIpad) => {
                    status.notice = Some("Pairing… confirm on the iPad if it asks".into());
                    let tx = ev_tx.clone();
                    tokio::task::spawn_local(async move { let _ = tx.send(Event::Paired(ipad::pair_usb(&ipad::ipad_dir()).await)); });
                }
                Some(ClipCommand::Phones(ids)) => hub.set_endpoints([Endpoint::Windows, Endpoint::Ipad].into_iter().chain(ids.into_iter().map(Endpoint::Peer)).collect()),
                Some(ClipCommand::FromPhone { id, text }) => writes = route(&mut hub, &history, &ev_tx, Endpoint::Peer(id), Content::Text(text)),
            },
            Some(c) = from_windows.recv() => writes = route(&mut hub, &history, &ev_tx, Endpoint::Windows, c),
            Some(ev) = events.recv() => match ev {
                Event::Connected(Ok(link)) => {
                    connecting = false;
                    session += 1;
                    diag::log(format_args!("iPad clipboard connected (wifi={})", link.wifi));
                    status.ipad = Some(IpadStatus::Connected { wifi: link.wifi });
                    writer = Some(link.writer);
                    let (mut reader, tx, s) = (link.reader, ev_tx.clone(), session);
                    if let Some(t) = reader_task.take() { t.abort(); }
                    reader_task = Some(tokio::task::spawn_local(async move {
                        loop {
                            let change = reader.next_change().await;
                            let end = change.is_err();
                            if tx.send(Event::FromIpad { session: s, change }).is_err() || end { break }
                        }
                    }));
                }
                Event::Connected(Err(e)) => {
                    connecting = false;
                    retry = tokio::time::Instant::now() + RECONNECT;
                    let s = ipad::classify(&e);
                    // Once per new reason, not every 2 s. Error texts never carry clipboard contents.
                    if status.ipad.as_ref() != Some(&s) { diag::log(format_args!("iPad clipboard not connected: {e}")); }
                    status.ipad = Some(s);
                }
                Event::FromIpad { session: s, .. } if s != session => {}
                Event::FromIpad { change: Ok(c), .. } => writes = route(&mut hub, &history, &ev_tx, Endpoint::Ipad, c),
                Event::FromIpad { change: Err(e), .. } => {
                    diag::log(format_args!("iPad clipboard disconnected: {e}"));
                    writer = None;
                    retry = tokio::time::Instant::now() + RECONNECT;
                    status.ipad = Some(ipad::classify(&e));
                }
                Event::Recorded => status.history_rev += 1,
                Event::Paired(r) => status.notice = Some(match r { Ok(()) => "iPad paired".into(), Err(e) => format!("Pairing failed: {e}") }),
            },
            _ = tokio::time::sleep_until(retry), if writer.is_none() && !connecting => {}
        }

        for (to, c) in writes {
            let ok = match to {
                Endpoint::Windows => {
                    let c2 = c.clone();
                    tokio::task::spawn_blocking(move || clipboard::write(&c2)).await.unwrap_or_else(|e| Err(e.to_string()))
                }
                Endpoint::Ipad => match writer.as_mut() {
                    // iPad down: drop it. The next copy anywhere wins anyway.
                    None => Err("iPad not connected".into()),
                    Some(w) => match tokio::time::timeout(write_timeout(c.size()), w.write(&c)).await {
                        Ok(r) => r,
                        Err(_) => Err("iPad write timed out".into()),
                    },
                },
                Endpoint::Peer(ref id) => match &c {
                    Content::Text(t) | Content::Rich { text: t, .. } => { to_phone(id.clone(), t.clone()); Ok(()) }
                    Content::Image(_) => continue, // phones get plain text only
                },
            };
            match ok {
                Ok(()) => { status.synced += 1; diag::log(format_args!("clip {} -> {to:?} ({} bytes)", c.kind(), c.size())); }
                Err(e) => {
                    diag::log(format_args!("clip {} -> {to:?} failed: {e}", c.kind()));
                    if to == Endpoint::Ipad && writer.is_some() {
                        writer = None;
                        session += 1;
                        if let Some(t) = reader_task.take() { t.abort(); }
                        retry = tokio::time::Instant::now() + RECONNECT;
                        status.ipad = Some(ipad::classify(&e));
                    }
                }
            }
        }
        status.skipped_too_large = hub.skipped_too_large();
    }
    watcher.stop();
}

fn route(hub: &mut Hub, history: &Arc<Mutex<History>>, ev: &UnboundedSender<Event>, from: Endpoint, c: Content) -> Vec<(Endpoint, Content)> {
    let c = c.fit();
    // Our own pairing code coming back is not a copy: it stays out of history.
    if !hub.is_pending_echo(&from, &c) && c.size() <= MAX_ITEM_BYTES { record(history, ev, &from, c.clone()); }
    hub.observe(from, c).into_iter().map(|(to, item)| (to, item.content)).collect()
}

/// Hashing, PNG encoding and the file writes happen on the blocking pool, never in this loop.
fn record(history: &Arc<Mutex<History>>, ev: &UnboundedSender<Event>, from: &Endpoint, c: Content) {
    let (history, ev) = (history.clone(), ev.clone());
    let from = match from { Endpoint::Windows => "This PC", Endpoint::Ipad => "iPad", Endpoint::Peer(_) => "Phone" };
    tokio::task::spawn_blocking(move || {
        let Some(item) = cliphistory::prepare(&c) else { return };
        let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        if history.lock().unwrap().add(from, item, at) { let _ = ev.send(Event::Recorded); }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn write_timeout_grows_with_size() { // Final review I5
        assert_eq!(write_timeout(10), Duration::from_secs(5));
        assert_eq!(write_timeout(33 * 1024 * 1024), Duration::from_secs(5 + 33));
    }
}
