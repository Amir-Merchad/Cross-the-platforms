//! Clipboard sync service: Windows clipboard ⇄ core `Hub` ⇄ iPad, on its own thread, separate from
//! the input engine so nothing here can ever stall keyboard/mouse. Logs kinds and sizes only.

use std::{thread, time::Duration};

use bridge_core::clipboard::{Content, Endpoint, Hub};
use serde::Serialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{clipboard, diag, ipad::{self, IpadLink, IpadStatus, IpadWriter}};

pub enum ClipCommand { SetEnabled(bool), PairIpad, Stop }

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ClipStatus {
    pub enabled: bool,
    pub ipad: Option<IpadStatus>,
    pub synced: u64,
    pub skipped_too_large: u64,
    pub notice: Option<String>,
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
const IPAD_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn start(enabled: bool, on_status: impl Fn(ClipStatus) + Send + 'static) -> ClipHandle {
    let (tx, rx) = unbounded_channel();
    let thread = thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
        // Single-threaded on purpose: idevice futures don't prove Send, and nothing here needs more threads.
        tokio::task::LocalSet::new().block_on(&rt, run(enabled, rx, on_status));
    });
    ClipHandle { tx, thread: Some(thread) }
}

/// Results of background work. `session` tags iPad events so a dead session's late errors are ignored.
enum Event {
    Connected(Result<Box<IpadLink>, String>),
    FromIpad { session: u64, change: Result<Content, String> },
    Paired(Result<(), String>),
}

async fn run(enabled: bool, mut commands: UnboundedReceiver<ClipCommand>, on_status: impl Fn(ClipStatus)) {
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
    let mut session = 0u64;
    let mut connecting = false;
    let mut retry = tokio::time::Instant::now();

    loop {
        if writer.is_none() && !connecting && tokio::time::Instant::now() >= retry {
            connecting = true;
            let tx = ev_tx.clone();
            tokio::task::spawn_local(async move { let _ = tx.send(Event::Connected(IpadLink::connect(&ipad::ipad_dir()).await.map(Box::new))); });
        }
        if reported.as_ref() != Some(&status) { reported = Some(status.clone()); on_status(status.clone()); }

        let mut writes: Vec<(Endpoint, Content)> = Vec::new();
        tokio::select! {
            c = commands.recv() => match c {
                None | Some(ClipCommand::Stop) => break,
                Some(ClipCommand::SetEnabled(on)) => { hub.set_paused(!on); status.enabled = on; }
                Some(ClipCommand::PairIpad) => {
                    status.notice = Some("Pairing… confirm on the iPad if it asks".into());
                    let tx = ev_tx.clone();
                    tokio::task::spawn_local(async move { let _ = tx.send(Event::Paired(ipad::pair_usb(&ipad::ipad_dir()).await)); });
                }
            },
            Some(c) = from_windows.recv() => writes = route(&mut hub, Endpoint::Windows, c),
            Some(ev) = events.recv() => match ev {
                Event::Connected(Ok(link)) => {
                    connecting = false;
                    session += 1;
                    diag::log(format_args!("iPad clipboard connected (wifi={})", link.wifi));
                    status.ipad = Some(IpadStatus::Connected { wifi: link.wifi });
                    writer = Some(link.writer);
                    let (mut reader, tx, s) = (link.reader, ev_tx.clone(), session);
                    tokio::task::spawn_local(async move {
                        loop {
                            let change = reader.next_change().await;
                            let end = change.is_err();
                            if tx.send(Event::FromIpad { session: s, change }).is_err() || end { break }
                        }
                    });
                }
                Event::Connected(Err(e)) => {
                    connecting = false;
                    retry = tokio::time::Instant::now() + RECONNECT;
                    status.ipad = Some(ipad::classify(&e));
                }
                Event::FromIpad { session: s, .. } if s != session => {}
                Event::FromIpad { change: Ok(c), .. } => writes = route(&mut hub, Endpoint::Ipad, c),
                Event::FromIpad { change: Err(e), .. } => {
                    diag::log(format_args!("iPad clipboard disconnected: {e}"));
                    writer = None;
                    retry = tokio::time::Instant::now() + RECONNECT;
                    status.ipad = Some(ipad::classify(&e));
                }
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
                    Some(w) => match tokio::time::timeout(IPAD_WRITE_TIMEOUT, w.write(&c)).await {
                        Ok(r) => r,
                        Err(_) => Err("iPad write timed out".into()),
                    },
                },
                Endpoint::Peer(_) => Err("no peers yet".into()),
            };
            match ok {
                Ok(()) => { status.synced += 1; diag::log(format_args!("clip {} -> {to:?} ({} bytes)", c.kind(), c.size())); }
                Err(e) => {
                    diag::log(format_args!("clip {} -> {to:?} failed: {e}", c.kind()));
                    if to == Endpoint::Ipad && writer.is_some() {
                        writer = None;
                        session += 1; // the reader of the broken session is ignored from now on
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

fn route(hub: &mut Hub, from: Endpoint, c: Content) -> Vec<(Endpoint, Content)> {
    hub.observe(from, c).into_iter().map(|(to, item)| (to, item.content)).collect()
}
