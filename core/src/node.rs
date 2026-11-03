//! A device on the network: persistent identity, paired peers, and the control channel that only
//! paired peers may use (clipboard text, file offers). File bytes travel separately (see `bulk`).

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use anyhow::Result;
use iroh::{Endpoint, EndpointAddr, PublicKey, endpoint::{Connection, RecvStream, SendStream, presets}};
use serde::{Deserialize, Serialize};
use tokio::{sync::{mpsc, oneshot}, task::JoinHandle, time::timeout};

use crate::{
    bulk::{self, BulkListener},
    identity::{PairedPeers, Peer, load_or_create_key},
    pairing::{self, PAIR_ALPN, PairInvite, PendingCode},
    transfer::fast_transport,
};

pub const CTL_ALPN: &[u8] = b"ctp/ctl/1";
/// File bytes over iroh when the LAN TLS stream can't connect (firewall, client isolation).
const BULK_ALPN: &[u8] = b"ctp/bulk/1";
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
/// How long a receiver waits for the iroh fallback after offering the TCP stream.
const FALLBACK_WAIT: Duration = Duration::from_secs(30);
/// Nodes tell their paired peers where they can be reached (iroh has no LAN discovery).
const ANNOUNCE_EVERY: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Big enough for a long clipboard text, small enough that a peer can't exhaust memory.
const MAX_CTL_LINE: usize = 4 << 20;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileMeta { pub name: String, pub size: u64 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Ctl {
    Hello { addr: EndpointAddr },
    Offer { id: u64, files: Vec<FileMeta> },
    Accept { id: u64, tcp: Vec<SocketAddr>, cert_sha256: String, token: String },
    Reject { id: u64, reason: String },
    Clip { text: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum Event {
    Paired(Peer),
    Incoming { from: String, files: Vec<FileMeta> },
    Progress { id: u64, done: u64, total: u64 },
    Received { from: String, path: PathBuf, size: u64 },
    Failed { id: u64, reason: String },
    Clip { from: String, text: String },
}

pub struct NodeConfig { pub data_dir: PathBuf, pub receive_dir: PathBuf, pub me: Peer }

#[derive(Clone)]
pub struct Node { inner: Arc<Inner> }

pub(crate) struct Inner {
    pub(crate) ep: Endpoint,
    me: Peer,
    receive_dir: Mutex<PathBuf>,
    peers: Mutex<PairedPeers>,
    peers_path: PathBuf,
    addrs: Mutex<HashMap<String, EndpointAddr>>,
    addrs_path: PathBuf,
    pending: Mutex<Option<PendingCode>>,
    pub(crate) events: mpsc::Sender<Event>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    /// Accepted offers waiting for the sender's iroh fallback stream: id → (sender id, hand-off).
    waiting: Mutex<HashMap<u64, (String, oneshot::Sender<IrohStream>)>>,
    test_bulk_addrs: Mutex<Option<Vec<SocketAddr>>>,
}

type IrohStream = (Connection, SendStream, RecvStream);

impl Drop for Inner {
    fn drop(&mut self) {
        for t in self.tasks.lock().unwrap().drain(..) { t.abort(); }
        let ep = self.ep.clone();
        if let Ok(rt) = tokio::runtime::Handle::try_current() { rt.spawn(async move { ep.close().await }); }
    }
}

impl Node {
    pub async fn start(cfg: NodeConfig) -> Result<(Node, mpsc::Receiver<Event>)> {
        let key = load_or_create_key(&cfg.data_dir.join("identity.key"))?;
        let ep = bind_endpoint(key, &cfg.data_dir.join("port")).await?;
        let (peers_path, addrs_path) = (cfg.data_dir.join("peers.json"), cfg.data_dir.join("addrs.json"));
        let addrs = std::fs::read_to_string(&addrs_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        let (tx, rx) = mpsc::channel(1024);
        let inner = Arc::new(Inner {
            ep: ep.clone(), me: cfg.me, receive_dir: Mutex::new(cfg.receive_dir),
            peers: Mutex::new(PairedPeers::load(&peers_path)), peers_path,
            addrs: Mutex::new(addrs), addrs_path,
            pending: Mutex::new(None), events: tx, tasks: Mutex::new(Vec::new()),
            waiting: Mutex::new(HashMap::new()), test_bulk_addrs: Mutex::new(None),
        });
        let node = Node { inner: inner.clone() };
        let accept = tokio::spawn(accept_loop(ep, Arc::downgrade(&inner)));
        let weak = Arc::downgrade(&inner);
        let announce = tokio::spawn(async move {
            loop {
                let Some(inner) = weak.upgrade() else { return };
                Node { inner }.announce_all().await;
                tokio::time::sleep(ANNOUNCE_EVERY).await;
            }
        });
        inner.tasks.lock().unwrap().extend([accept, announce]);
        Ok((node, rx))
    }

    /// Stops background tasks and closes the endpoint (frees the port for the next start).
    pub async fn shutdown(&self) {
        for t in self.inner.tasks.lock().unwrap().drain(..) { t.abort(); }
        self.inner.ep.close().await;
    }

    pub fn id(&self) -> String { self.inner.ep.id().to_string() }
    pub fn addr(&self) -> EndpointAddr { self.inner.ep.addr() }
    /// Where the next accepted transfer lands.
    pub fn set_receive_dir(&self, dir: PathBuf) { *self.inner.receive_dir.lock().unwrap() = dir; }
    pub fn peers(&self) -> Vec<Peer> { self.inner.peers.lock().unwrap().list().to_vec() }

    pub fn forget(&self, id: &str) {
        let mut p = self.inner.peers.lock().unwrap();
        p.remove(id);
        let _ = p.save(&self.inner.peers_path);
    }

    /// A fresh one-time invite (replaces any previous one) to show as a QR code.
    pub fn invite(&self) -> PairInvite {
        let pending = PendingCode::new(Instant::now());
        let code = pending.code().to_string();
        *self.inner.pending.lock().unwrap() = Some(pending);
        PairInvite { v: 1, name: self.inner.me.name.clone(), addr: pairing::compact_addr(&self.inner.ep.addr(), bulk::primary_ipv4()), code }
    }

    /// Scans side: pair with the device that showed `invite`.
    pub async fn join(&self, invite: &PairInvite) -> Result<Peer, String> {
        let host = pairing::join(&self.inner.ep, invite, &self.inner.me).await?;
        self.inner.remember_addr(&host.id, invite.addr.clone());
        self.inner.add_peer(host.clone());
        let node = self.clone();
        let id = host.id.clone();
        tokio::spawn(async move { let _ = node.send_ctl(&id, &[]).await; }); // tell the host where we are
        Ok(host)
    }

    pub async fn send_clip(&self, peer_id: &str, text: String) -> Result<(), String> {
        self.send_ctl(peer_id, &[Ctl::Clip { text }]).await.map(|_| ())
    }

    /// Offers `paths` to a paired device and streams them once it accepts: LAN TLS first, iroh if
    /// that can't connect within a few seconds. Returns the bytes sent.
    pub async fn send_files(&self, peer_id: &str, paths: Vec<PathBuf>) -> Result<u64, String> {
        let mut files = Vec::with_capacity(paths.len());
        for p in &paths {
            let m = tokio::fs::metadata(p).await.map_err(|e| format!("can't read a file: {e}"))?;
            if !m.is_file() { return Err("only files can be sent".into()) }
            files.push(FileMeta { name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), size: m.len() });
        }
        let id = random_id();
        let replies = self.send_ctl(peer_id, &[Ctl::Offer { id, files }]).await?;
        let (tcp, cert, token) = match replies.into_iter().find_map(|c| match c {
            Ctl::Accept { id: i, tcp, cert_sha256, token } if i == id => Some(Ok((tcp, cert_sha256, token))),
            Ctl::Reject { id: i, reason } if i == id => Some(Err(reason)),
            _ => None,
        }) {
            Some(Ok(a)) => a,
            Some(Err(reason)) => return Err(format!("declined: {reason}")),
            None => return Err("the device didn't answer the offer".into()),
        };
        let progress = progress_events(self.inner.events.clone(), id);
        let result = match bulk::connect(&tcp, &cert).await {
            Ok(mut s) => bulk::send_on(&mut s, &token, &paths, progress).await,
            Err(_) => self.send_over_iroh(peer_id, id, &token, &paths, progress).await,
        };
        result.map_err(|e| {
            let reason = e.to_string();
            let _ = self.inner.events.try_send(Event::Failed { id, reason: reason.clone() });
            reason
        })
    }

    async fn send_over_iroh(&self, peer_id: &str, id: u64, token: &str, paths: &[PathBuf], progress: impl FnMut(u64, u64)) -> Result<u64> {
        let conn = self.connect(peer_id, BULK_ALPN).await.map_err(anyhow::Error::msg)?;
        let (mut s, r) = conn.open_bi().await?;
        s.write_all(&id.to_le_bytes()).await?;
        let mut stream = tokio::io::join(r, s);
        let n = bulk::send_on(&mut stream, token, paths, progress).await?;
        conn.close(0u32.into(), b"done");
        Ok(n)
    }

    /// Test hook: the addresses this node puts in `Accept` instead of its real ones.
    #[doc(hidden)]
    pub fn test_bulk_addrs(&self, addrs: Option<Vec<SocketAddr>>) { *self.inner.test_bulk_addrs.lock().unwrap() = addrs; }

    async fn connect(&self, peer_id: &str, alpn: &[u8]) -> Result<Connection, String> {
        if !self.inner.peers.lock().unwrap().contains(peer_id) { return Err("not paired with that device".into()) }
        let addr = self.inner.addr_for(peer_id)?;
        match timeout(CONNECT_TIMEOUT, self.inner.ep.connect(addr, alpn)).await {
            Ok(Ok(c)) => Ok(c),
            // The saved address may be stale (new network, new IP): let n0 discovery find it.
            _ => {
                let id_only = PublicKey::from_str(peer_id).map(EndpointAddr::from).map_err(|e| format!("bad device id: {e}"))?;
                timeout(CONNECT_TIMEOUT, self.inner.ep.connect(id_only, alpn)).await
                    .map_err(|_| "device not reachable".to_string())?.map_err(|e| format!("device not reachable: {e}"))
            }
        }
    }

    /// One control exchange: our `Hello`, then `msgs`; returns the peer's reply lines.
    pub(crate) async fn send_ctl(&self, peer_id: &str, msgs: &[Ctl]) -> Result<Vec<Ctl>, String> {
        let conn = self.connect(peer_id, CTL_ALPN).await?;
        let (mut s, mut r) = conn.open_bi().await.map_err(|e| e.to_string())?;
        let mut out = String::new();
        for m in std::iter::once(&Ctl::Hello { addr: self.inner.ep.addr() }).chain(msgs) {
            out.push_str(&serde_json::to_string(m).map_err(|e| e.to_string())?);
            out.push('\n');
        }
        s.write_all(out.as_bytes()).await.map_err(|e| e.to_string())?;
        s.finish().map_err(|e| e.to_string())?;
        let reply = r.read_to_end(MAX_CTL_LINE).await.map_err(|e| e.to_string())?;
        conn.close(0u32.into(), b"");
        Ok(reply.split(|&b| b == b'\n').filter(|l| !l.is_empty()).filter_map(|l| serde_json::from_slice(l).ok()).collect())
    }

    async fn announce_all(&self) {
        for p in self.peers() { let _ = self.send_ctl(&p.id, &[]).await; }
    }

}

impl Inner {
    fn add_peer(&self, p: Peer) {
        let mut peers = self.peers.lock().unwrap();
        peers.add(p);
        let _ = peers.save(&self.peers_path);
    }

    fn remember_addr(&self, id: &str, addr: EndpointAddr) {
        let mut a = self.addrs.lock().unwrap();
        if a.get(id) == Some(&addr) { return }
        a.insert(id.to_string(), addr);
        save_json(&self.addrs_path, &*a);
    }

    fn addr_for(&self, id: &str) -> Result<EndpointAddr, String> {
        if let Some(a) = self.addrs.lock().unwrap().get(id) { return Ok(a.clone()) }
        // No address yet: the id alone lets iroh look the device up via n0's discovery service.
        PublicKey::from_str(id).map(EndpointAddr::from).map_err(|e| format!("bad device id: {e}"))
    }

    pub(crate) fn is_paired(&self, id: &str) -> bool { self.peers.lock().unwrap().contains(id) }
}

/// Binds on the port this device used last time, so the addresses its peers saved stay valid
/// across restarts; a random port (saved for next time) if that one is taken.
async fn bind_endpoint(key: iroh::SecretKey, port_file: &Path) -> Result<Endpoint> {
    let alpns = vec![PAIR_ALPN.to_vec(), CTL_ALPN.to_vec(), BULK_ALPN.to_vec()];
    let saved: Option<u16> = std::fs::read_to_string(port_file).ok().and_then(|t| t.trim().parse().ok());
    if let Some(port) = saved {
        let fixed = Endpoint::builder(presets::N0).secret_key(key.clone()).alpns(alpns.clone()).transport_config(fast_transport()).clear_ip_transports()
            .bind_addr(SocketAddr::from(([0, 0, 0, 0], port)))
            .and_then(|b| b.bind_addr(SocketAddr::from(([0u16; 8], port))));
        if let Ok(b) = fixed && let Ok(ep) = b.bind().await { return Ok(ep) }
    }
    let ep = Endpoint::builder(presets::N0).secret_key(key).alpns(alpns).transport_config(fast_transport()).bind().await?;
    if let Some(port) = ep.bound_sockets().iter().find(|a| a.is_ipv4()).map(|a| a.port()) {
        if let Some(d) = port_file.parent() { let _ = std::fs::create_dir_all(d); }
        let _ = std::fs::write(port_file, port.to_string());
    }
    Ok(ep)
}

fn save_json(path: &Path, v: &impl Serialize) {
    if let Some(d) = path.parent() { let _ = std::fs::create_dir_all(d); }
    if let Ok(t) = serde_json::to_string_pretty(v) { let _ = std::fs::write(path, t); }
}

async fn accept_loop(ep: Endpoint, inner: Weak<Inner>) {
    while let Some(incoming) = ep.accept().await {
        let inner = inner.clone();
        tokio::spawn(async move {
            let Ok(conn) = incoming.await else { return };
            let Some(inner) = inner.upgrade() else { return };
            let alpn = conn.alpn().to_vec();
            if alpn == PAIR_ALPN { handle_pair(conn, &inner).await }
            else if alpn == CTL_ALPN { handle_ctl(conn, Node { inner }).await }
            else if alpn == BULK_ALPN { handle_bulk(conn, &inner).await }
            else { conn.close(1u32.into(), b"unknown protocol") }
        });
    }
}

async fn handle_pair(conn: Connection, inner: &Inner) {
    // Only while an invite is open; the code is taken out for the exchange and put back after.
    let Some(mut pending) = inner.pending.lock().unwrap().take() else { conn.close(1u32.into(), b"no invite open"); return };
    let me = inner.me.clone();
    let result = pairing::answer(&conn, &mut pending, &me, Instant::now()).await;
    *inner.pending.lock().unwrap() = Some(pending);
    if let Ok(peer) = result {
        inner.add_peer(peer.clone());
        let _ = inner.events.send(Event::Paired(peer)).await;
    }
}

async fn handle_ctl(conn: Connection, node: Node) {
    let from = conn.remote_id().to_string();
    if !node.inner.is_paired(&from) { conn.close(1u32.into(), b"not paired"); return }
    let Ok((mut s, mut r)) = conn.accept_bi().await else { return };
    let mut replies = String::new();
    // The sender finishes its side after its messages: read all of it, then handle line by line.
    let Ok(data) = r.read_to_end(MAX_CTL_LINE).await else { return };
    for line in data.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
        let Ok(msg) = serde_json::from_slice::<Ctl>(line) else { break };
        if let Some(reply) = node.on_ctl(&from, msg).await {
            replies.push_str(&serde_json::to_string(&reply).unwrap_or_default());
            replies.push('\n');
        }
    }
    let _ = s.write_all(replies.as_bytes()).await;
    let _ = s.finish();
    let _ = timeout(Duration::from_secs(5), conn.closed()).await;
}

impl Node {
    /// Handles one control message from a paired peer; returns the reply line, if any.
    async fn on_ctl(&self, from: &str, msg: Ctl) -> Option<Ctl> {
        match msg {
            Ctl::Hello { addr } => { self.inner.remember_addr(from, addr); None }
            Ctl::Clip { text } => { let _ = self.inner.events.send(Event::Clip { from: from.to_string(), text }).await; None }
            Ctl::Offer { id, files } => Some(self.accept_offer(from, id, files).await),
            Ctl::Accept { .. } | Ctl::Reject { .. } => None,
        }
    }

    /// Paired peers' offers are accepted automatically: bind a one-shot TLS listener, wait for the
    /// sender there or on the iroh fallback, write into the receive dir.
    async fn accept_offer(&self, from: &str, id: u64, files: Vec<FileMeta>) -> Ctl {
        let l = match BulkListener::bind().await { Ok(l) => l, Err(e) => return Ctl::Reject { id, reason: format!("can't receive: {e}") } };
        let tcp = self.inner.test_bulk_addrs.lock().unwrap().clone().unwrap_or_else(|| l.addrs());
        let (cert_sha256, token) = (l.cert_sha256(), l.token());
        let (tx, rx) = oneshot::channel();
        self.inner.waiting.lock().unwrap().insert(id, (from.to_string(), tx));
        let _ = self.inner.events.send(Event::Incoming { from: from.to_string(), files: files.clone() }).await;
        tokio::spawn(self.clone().receive_files(from.to_string(), id, files, l, token.clone(), rx));
        Ctl::Accept { id, tcp, cert_sha256, token }
    }

    async fn receive_files(self, from: String, id: u64, files: Vec<FileMeta>, l: BulkListener, token: String, mut iroh: oneshot::Receiver<IrohStream>) {
        let dir = self.inner.receive_dir.lock().unwrap().clone();
        let events = self.inner.events.clone();
        let over_iroh = |(conn, s, r): IrohStream| {
            let (files, dir, token, events) = (files.clone(), dir.clone(), token.clone(), events.clone());
            async move {
                let mut stream = tokio::io::join(r, s);
                let out = bulk::receive_on(&mut stream, &token, &files, &dir, progress_events(events, id)).await;
                let _ = timeout(Duration::from_secs(2), conn.closed()).await;
                out
            }
        };
        let mut tcp = Box::pin(l.receive(&files, &dir, progress_events(events.clone(), id)));
        let result = tokio::select! {
            r = &mut tcp => match r {
                Ok(paths) => Ok(paths),
                // A failed TLS attempt (a stranger, a pinning mismatch) still leaves the iroh path.
                Err(e) => match timeout(FALLBACK_WAIT, &mut iroh).await { Ok(Ok(st)) => over_iroh(st).await, _ => Err(e) },
            },
            st = &mut iroh => { drop(tcp); match st { Ok(st) => over_iroh(st).await, Err(_) => Err(anyhow::anyhow!("transfer abandoned")) } }
        };
        self.inner.waiting.lock().unwrap().remove(&id);
        match result {
            Ok(paths) => for (path, f) in paths.into_iter().zip(&files) {
                let _ = events.send(Event::Received { from: from.clone(), path, size: f.size }).await;
            },
            Err(e) => { let _ = events.send(Event::Failed { id, reason: e.to_string() }).await; }
        }
    }
}

/// The sender's iroh fallback stream: it names the offer id first; only that offer's peer may use it.
async fn handle_bulk(conn: Connection, inner: &Inner) {
    let from = conn.remote_id().to_string();
    if !inner.is_paired(&from) { conn.close(1u32.into(), b"not paired"); return }
    let Ok((s, mut r)) = conn.accept_bi().await else { return };
    let mut id = [0u8; 8];
    if r.read_exact(&mut id).await.is_err() { return }
    let id = u64::from_le_bytes(id);
    let tx = {
        let mut waiting = inner.waiting.lock().unwrap();
        match waiting.get(&id) { Some((peer, _)) if *peer == from => waiting.remove(&id).map(|(_, tx)| tx), _ => None }
    };
    match tx { Some(tx) => { let _ = tx.send((conn, s, r)); } None => conn.close(1u32.into(), b"no such transfer") }
}

/// Progress as events, at most every `PROGRESS_EVERY` (plus the final one).
fn progress_events(events: mpsc::Sender<Event>, id: u64) -> impl FnMut(u64, u64) + Send {
    let mut last: Option<Instant> = None;
    move |done, total| {
        let now = Instant::now();
        if done == total || last.is_none_or(|t| now - t >= PROGRESS_EVERY) {
            last = Some(now);
            let _ = events.try_send(Event::Progress { id, done, total });
        }
    }
}

fn random_id() -> u64 {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("OS random source");
    u64::from_le_bytes(b)
}
