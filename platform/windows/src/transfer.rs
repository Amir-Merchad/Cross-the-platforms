//! File transfer service: owns the core `Node` on its own tokio runtime, separate from the input
//! engine and the clipboard thread. Events go out through `on_event`; never logs file names.

use std::{future::Future, path::PathBuf, sync::Mutex, time::Duration};

use bridge_core::{
    identity::{Peer, PeerKind},
    node::{Event, Node, NodeConfig, SendOutcome},
    pairing::PairInvite,
};

use crate::diag;

/// `%APPDATA%\CrossThePlatforms\node` — identity key, paired devices, address book.
pub fn data_dir() -> PathBuf { crate::engine::settings_path().with_file_name("node") }

/// `%USERPROFILE%\Downloads\Bridge`
pub fn default_receive_dir() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())).join("Downloads").join("Bridge")
}

pub struct TransferHandle { rt: tokio::runtime::Runtime, node: Node }

/// The running service, for the send hotkeys: the hub owns the handle and the engine starts before it.
static RUNNING: Mutex<Option<(tokio::runtime::Handle, Node)>> = Mutex::new(None);

fn spawn_send(rt: &tokio::runtime::Handle, node: Node, peer_id: String, paths: Vec<PathBuf>) {
    rt.spawn(async move {
        let n = paths.len();
        match node.send_or_queue(&peer_id, paths).await {
            Ok(SendOutcome::Sent(bytes)) => diag::log(format_args!("sent {n} file(s), {bytes} bytes")),
            Ok(SendOutcome::Queued) => diag::log(format_args!("queued {n} file(s) for later")),
            Err(e) => diag::log(format_args!("send failed: {e}")),
        }
    });
}

/// `send_files` from a worker thread. False when the service is not running.
pub fn send_files_to(peer_id: String, paths: Vec<PathBuf>) -> bool {
    let Some((rt, node)) = RUNNING.lock().unwrap().clone() else { return false };
    spawn_send(&rt, node, peer_id, paths);
    true
}

/// A later answer would press paste into whatever has focus by then.
const CLIP_ANSWER: Duration = Duration::from_secs(5);
/// ponytail: a guess at how long the app takes to write its clipboard. The peer answers when the clip is
/// queued, not when it is written, so a slower app pastes the old clipboard. Upgrade: a reply once it has written.
const CLIP_SETTLE: Duration = Duration::from_millis(300);

async fn answered(send: impl Future<Output = Result<(), String>>, limit: Duration, settle: Duration) -> Result<(), String> {
    match tokio::time::timeout(limit, send).await {
        Ok(Ok(())) => { tokio::time::sleep(settle).await; Ok(()) }
        Ok(Err(e)) => { diag::log(format_args!("clip to phone failed: {e}")); Err(crate::clipsync::NO_ANSWER.into()) }
        Err(_) => { diag::log("clip to phone timed out"); Err(crate::clipsync::NO_ANSWER.into()) }
    }
}

/// Sends clipboard text, then says whether the device took it, after a short wait for its app to write it.
pub fn send_clip_then(peer_id: String, text: String, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    let Some((rt, node)) = RUNNING.lock().unwrap().clone() else { return done(Err(crate::clipsync::NO_ANSWER.into())) };
    rt.spawn(async move { done(answered(node.send_clip(&peer_id, text), CLIP_ANSWER, CLIP_SETTLE).await) });
}

impl TransferHandle {
    pub fn start(receive_dir: PathBuf, on_event: impl Fn(Event) + Send + 'static) -> Result<Self, String> {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).thread_name("ctp-transfer").enable_all().build().map_err(|e| e.to_string())?;
        let name = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".into());
        let me = Peer { id: String::new(), name, kind: PeerKind::Windows };
        let (node, mut rx) = rt.block_on(Node::start(NodeConfig { data_dir: data_dir(), receive_dir, me })).map_err(|e| format!("{e:#}"))?;
        rt.spawn(async move {
            while let Some(e) = rx.recv().await {
                match &e {
                    Event::Received { size, .. } => diag::log(format_args!("file received ({size} bytes)")),
                    Event::Failed { reason, .. } => diag::log(format_args!("transfer failed: {reason}")),
                    _ => {}
                }
                on_event(e);
            }
        });
        *RUNNING.lock().unwrap() = Some((rt.handle().clone(), node.clone()));
        Ok(Self { rt, node })
    }

    pub fn invite(&self) -> PairInvite { self.node.invite() }
    pub fn peers(&self) -> Vec<Peer> { self.node.peers() }
    /// Peers heard from in the last 150 s (two and a half announce periods).
    pub fn reachable(&self) -> Vec<String> { self.node.reachable(std::time::Duration::from_secs(150)) }
    pub fn waiting(&self) -> Vec<(String, usize)> { self.node.waiting() }
    pub fn forget(&self, id: &str) { self.node.forget(id) }
    pub fn set_receive_dir(&self, dir: PathBuf) { self.node.set_receive_dir(dir) }

    /// Runs in the background; progress and the outcome arrive as events.
    pub fn send_files(&self, peer_id: String, paths: Vec<PathBuf>) {
        spawn_send(self.rt.handle(), self.node.clone(), peer_id, paths);
    }

    pub fn send_clip(&self, peer_id: String, text: String) {
        let node = self.node.clone();
        self.rt.spawn(async move { if let Err(e) = node.send_clip(&peer_id, text).await { diag::log(format_args!("clip to phone failed: {e}")) } });
    }
}

impl Drop for TransferHandle {
    fn drop(&mut self) {
        *RUNNING.lock().unwrap() = None;
        let node = self.node.clone();
        let _ = self.rt.block_on(async { tokio::time::timeout(std::time::Duration::from_secs(2), node.shutdown()).await });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{future::{pending, ready}, time::Instant};
    use crate::clipsync::NO_ANSWER;

    const LONG: Duration = Duration::from_secs(60);
    const SHORT: Duration = Duration::from_millis(20);

    #[tokio::test] async fn a_phone_that_never_answers_is_no_answer() { // Review I2: no late paste
        assert_eq!(answered(pending(), SHORT, Duration::ZERO).await, Err(NO_ANSWER.to_string()));
    }
    #[tokio::test] async fn a_refused_clip_is_no_answer_at_once() {
        let t = Instant::now();
        assert_eq!(answered(ready(Err("no route".to_string())), LONG, LONG).await, Err(NO_ANSWER.to_string()));
        assert!(t.elapsed() < SHORT * 50, "no settling wait after a failure");
    }
    #[tokio::test] async fn a_taken_clip_waits_for_the_app_to_write_it() { // Review I1
        let t = Instant::now();
        assert_eq!(answered(ready(Ok(())), LONG, SHORT).await, Ok(()));
        assert!(t.elapsed() >= SHORT);
    }
}
