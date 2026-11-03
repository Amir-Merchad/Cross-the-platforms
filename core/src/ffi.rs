//! UniFFI surface for the Kotlin and Swift apps. Thin wrappers; logic stays in the modules.

use std::sync::Arc;

use crate::transfer::{Receiver, send_file};

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

#[derive(uniffi::Record)]
pub struct FfiReceived { pub path: String, pub size: u64 }

#[derive(uniffi::Object)]
pub struct FfiReceiver { inner: tokio::sync::Mutex<Receiver>, ticket: String }

#[uniffi::export(async_runtime = "tokio")]
impl FfiReceiver {
    pub fn ticket(&self) -> String { self.ticket.clone() }

    pub async fn next(&self) -> Result<FfiReceived, FfiError> {
        let r = self.inner.lock().await.next().await?;
        Ok(FfiReceived { path: r.path.to_string_lossy().into_owned(), size: r.size })
    }
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn start_receiver(dir: String) -> Result<Arc<FfiReceiver>, FfiError> {
    let rx = Receiver::start(dir.into()).await?;
    let ticket = rx.ticket();
    Ok(Arc::new(FfiReceiver { inner: tokio::sync::Mutex::new(rx), ticket }))
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn send_file_ffi(ticket: String, path: String) -> Result<u64, FfiError> {
    Ok(send_file(&ticket, path.as_ref()).await?)
}
