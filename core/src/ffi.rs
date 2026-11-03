//! UniFFI surface for the Kotlin and Swift apps. Thin wrappers; logic stays in `node`.

use std::sync::Arc;

use crate::{
    identity::{Peer, PeerKind},
    node::{Event, FileMeta, Node, NodeConfig},
    pairing::PairInvite,
};

#[derive(Debug, uniffi::Error)]
pub enum FfiError { Failed { reason: String } }

impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { FfiError::Failed { reason } => f.write_str(reason) }
    }
}

impl From<anyhow::Error> for FfiError {
    fn from(e: anyhow::Error) -> Self { FfiError::Failed { reason: format!("{e:#}") } }
}
impl From<String> for FfiError {
    fn from(reason: String) -> Self { FfiError::Failed { reason } }
}

#[derive(uniffi::Record)]
pub struct FfiPeer { pub id: String, pub name: String, pub kind: PeerKind }
impl From<Peer> for FfiPeer { fn from(p: Peer) -> Self { FfiPeer { id: p.id, name: p.name, kind: p.kind } } }

#[derive(uniffi::Record)]
pub struct FfiFile { pub name: String, pub size: u64 }
impl From<FileMeta> for FfiFile { fn from(f: FileMeta) -> Self { FfiFile { name: f.name, size: f.size } } }

#[derive(uniffi::Enum)]
pub enum FfiEvent {
    Paired { peer: FfiPeer },
    Incoming { from: String, files: Vec<FfiFile> },
    Progress { id: u64, done: u64, total: u64 },
    Received { from: String, path: String, size: u64 },
    Failed { id: u64, reason: String },
    Clip { from: String, text: String },
    Queued { peer: String, files: u64 },
}

impl From<Event> for FfiEvent {
    fn from(e: Event) -> Self {
        match e {
            Event::Paired(p) => FfiEvent::Paired { peer: p.into() },
            Event::Incoming { from, files } => FfiEvent::Incoming { from, files: files.into_iter().map(Into::into).collect() },
            Event::Progress { id, done, total } => FfiEvent::Progress { id, done, total },
            Event::Received { from, path, size } => FfiEvent::Received { from, path: path.to_string_lossy().into_owned(), size },
            Event::Failed { id, reason } => FfiEvent::Failed { id, reason },
            Event::Clip { from, text } => FfiEvent::Clip { from, text },
            Event::Queued { peer, files } => FfiEvent::Queued { peer, files: files as u64 },
        }
    }
}

/// Implemented by the app; called from a background thread for every node event.
#[uniffi::export(callback_interface)]
pub trait NodeEvents: Send + Sync { fn on_event(&self, e: FfiEvent); }

#[derive(uniffi::Object)]
pub struct FfiNode { node: Node }

#[uniffi::export(async_runtime = "tokio")]
pub async fn start_node(data_dir: String, receive_dir: String, name: String, kind: PeerKind, events: Box<dyn NodeEvents>) -> Result<Arc<FfiNode>, FfiError> {
    let (node, mut rx) = Node::start(NodeConfig { data_dir: data_dir.into(), receive_dir: receive_dir.into(), me: Peer { id: String::new(), name, kind } }).await?;
    tokio::spawn(async move { while let Some(e) = rx.recv().await { events.on_event(e.into()) } });
    Ok(Arc::new(FfiNode { node }))
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiNode {
    pub fn id(&self) -> String { self.node.id() }
    pub fn peers(&self) -> Vec<FfiPeer> { self.node.peers().into_iter().map(Into::into).collect() }
    pub fn forget(&self, id: String) { self.node.forget(&id) }

    /// Pairs with the device whose QR code was scanned.
    pub async fn join(&self, qr_text: String) -> Result<FfiPeer, FfiError> {
        let invite = PairInvite::parse(&qr_text)?;
        Ok(self.node.join(&invite).await?.into())
    }
    pub async fn send_files(&self, peer_id: String, paths: Vec<String>) -> Result<u64, FfiError> {
        Ok(self.node.send_files(&peer_id, paths.into_iter().map(Into::into).collect()).await?)
    }
    pub async fn send_clip(&self, peer_id: String, text: String) -> Result<(), FfiError> {
        Ok(self.node.send_clip(&peer_id, text).await?)
    }
    pub async fn shutdown(&self) { self.node.shutdown().await }
}
