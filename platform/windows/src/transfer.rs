//! File transfer service: owns the core `Node` on its own tokio runtime, separate from the input
//! engine and the clipboard thread. Events go out through `on_event`; never logs file names.

use std::path::PathBuf;

use bridge_core::{
    identity::{Peer, PeerKind},
    node::{Event, Node, NodeConfig},
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
        Ok(Self { rt, node })
    }

    pub fn invite(&self) -> PairInvite { self.node.invite() }
    pub fn peers(&self) -> Vec<Peer> { self.node.peers() }
    pub fn forget(&self, id: &str) { self.node.forget(id) }
    pub fn set_receive_dir(&self, dir: PathBuf) { self.node.set_receive_dir(dir) }

    /// Runs in the background; progress and the outcome arrive as events.
    pub fn send_files(&self, peer_id: String, paths: Vec<PathBuf>) {
        let node = self.node.clone();
        self.rt.spawn(async move {
            let n = paths.len();
            match node.send_files(&peer_id, paths).await {
                Ok(bytes) => diag::log(format_args!("sent {n} file(s), {bytes} bytes")),
                Err(e) => diag::log(format_args!("send failed: {e}")),
            }
        });
    }

    pub fn send_clip(&self, peer_id: String, text: String) {
        let node = self.node.clone();
        self.rt.spawn(async move { if let Err(e) = node.send_clip(&peer_id, text).await { diag::log(format_args!("clip to phone failed: {e}")) } });
    }
}

impl Drop for TransferHandle {
    fn drop(&mut self) {
        let node = self.node.clone();
        let _ = self.rt.block_on(async { tokio::time::timeout(std::time::Duration::from_secs(2), node.shutdown()).await });
    }
}
