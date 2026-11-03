//! File transfer over iroh (QUIC, key-addressed). One connection + one bi-stream per file.
//! Wire (v1): sender writes `{"v":1,"name":..,"size":..}\n` then the bytes and finishes;
//! receiver replies the SHA-256 hex + `\n` only after all `size` bytes arrived.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use iroh::{Endpoint, EndpointAddr, endpoint::presets};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{fs, io::{AsyncReadExt, AsyncWriteExt}};

use crate::names::safe_name;

pub const ALPN: &[u8] = b"ctp/transfer/1";
const MAX_HEADER: usize = 4096;

#[derive(Serialize, Deserialize)]
struct Header { v: u32, name: String, size: u64 }

pub struct Received { pub path: PathBuf, pub size: u64 }

pub struct Receiver { ep: Endpoint, dir: PathBuf }

impl Receiver {
    pub async fn start(dir: PathBuf) -> Result<Receiver> {
        fs::create_dir_all(&dir).await?;
        let ep = Endpoint::builder(presets::N0).transport_config(fast_transport()).alpns(vec![ALPN.to_vec()]).bind().await?;
        Ok(Receiver { ep, dir })
    }

    /// The receiver's address as a string to share (QR / paste). JSON-encoded `EndpointAddr`.
    pub fn ticket(&self) -> String { serde_json::to_string(&self.ep.addr()).expect("addr serializes") }

    /// Waits for the next incoming file. A failed or cut-off transfer leaves nothing behind.
    pub async fn next(&mut self) -> Result<Received> {
        let conn = self.ep.accept().await.context("endpoint closed")?.await?;
        let (mut send, mut recv) = conn.accept_bi().await?;

        let mut head = Vec::new();
        let mut buf = vec![0u8; 1024 * 1024];
        let rest = loop {
            let n = recv.read(&mut buf).await?.context("stream ended before header")?;
            head.extend_from_slice(&buf[..n]);
            if let Some(i) = head.iter().position(|&b| b == b'\n') { let rest = head.split_off(i + 1); head.pop(); break rest }
            ensure!(head.len() <= MAX_HEADER, "header too long");
        };
        let h: Header = serde_json::from_slice(&head).context("bad header")?;
        ensure!(h.v == 1, "unsupported version {}", h.v);

        let path = safe_name(&self.dir, &h.name);
        let part = path.with_file_name(format!("{}.part", path.file_name().unwrap().to_string_lossy()));
        let result = async {
            let mut f = fs::File::create(&part).await?;
            let mut hasher = Sha256::new();
            let mut got = rest.len() as u64;
            hasher.update(&rest);
            f.write_all(&rest).await?;
            while let Some(n) = recv.read(&mut buf).await? {
                got += n as u64;
                ensure!(got <= h.size, "more bytes than announced");
                hasher.update(&buf[..n]);
                f.write_all(&buf[..n]).await?;
            }
            ensure!(got == h.size, "transfer cut off at {got}/{} bytes", h.size);
            f.flush().await?;
            drop(f);
            fs::rename(&part, &path).await?;
            Ok::<_, anyhow::Error>(hex(&hasher.finalize()))
        }.await;
        match result {
            Ok(digest) => {
                send.write_all(format!("{digest}\n").as_bytes()).await?;
                send.finish()?;
                conn.closed().await;
                Ok(Received { path, size: h.size })
            }
            Err(e) => { let _ = fs::remove_file(&part).await; Err(e) }
        }
    }
}

/// Sends one file; returns bytes sent. Errors if the receiver's hash doesn't match.
pub async fn send_file(ticket: &str, path: &Path) -> Result<u64> {
    let addr: EndpointAddr = serde_json::from_str(ticket).context("bad ticket")?;
    let size = fs::metadata(path).await?.len();
    let name = path.file_name().context("no file name")?.to_string_lossy().into_owned();
    let ep = Endpoint::builder(presets::N0).transport_config(fast_transport()).bind().await?;
    let conn = ep.connect(addr, ALPN).await?;
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(serde_json::to_string(&Header { v: 1, name, size })?.as_bytes()).await?;
    send.write_all(b"\n").await?;

    let mut f = fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 { break }
        hasher.update(&buf[..n]);
        send.write_all(&buf[..n]).await?;
    }
    send.finish()?;
    let reply = recv.read_to_end(128).await?;
    let ours = hex(&hasher.finalize());
    if String::from_utf8_lossy(&reply).trim() != ours { bail!("receiver hash mismatch") }
    conn.close(0u32.into(), b"done");
    ep.close().await;
    Ok(size)
}

/// Larger flow-control windows than Quinn's defaults (~1.25 MB per stream), so one bulk stream
/// isn't throttled by window updates on Wi-Fi. Values from env for tuning: CTP_STREAM_WIN_MB / CTP_SEND_WIN_MB.
pub(crate) fn fast_transport() -> iroh::endpoint::QuicTransportConfig {
    let mb = |name: &str, default: u64| std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default) * 1024 * 1024;
    let stream = mb("CTP_STREAM_WIN_MB", 16);
    let send = mb("CTP_SEND_WIN_MB", 64);
    iroh::endpoint::QuicTransportConfig::builder()
        .stream_receive_window(iroh::endpoint::VarInt::from_u64(stream).expect("window fits"))
        .receive_window(iroh::endpoint::VarInt::from_u64(send).expect("window fits"))
        .send_window(send)
        .build()
}

fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
