//! Pairing: the hub shows an invite as a QR code; the phone scans it and proves it saw the code.
//! The code is random, single use and valid for 5 minutes. Peer ids come from the authenticated
//! iroh connection, never from what the other side claims.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::identity::{Peer, PeerKind};

pub const PAIR_ALPN: &[u8] = b"ctp/pair/1";
const QR_PREFIX: &str = "ctp-pair:";
const CODE_LIFETIME: Duration = Duration::from_secs(300);
const MAX_LINE: usize = 4096;
const HELLO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PairInvite { pub v: u32, pub name: String, pub addr: iroh::EndpointAddr, pub code: String }

impl PairInvite {
    pub fn to_qr_text(&self) -> String { format!("{QR_PREFIX}{}", serde_json::to_string(self).expect("invite serializes")) }

    pub fn parse(text: &str) -> Result<Self, String> {
        let json = text.trim().strip_prefix(QR_PREFIX).ok_or("not a Cross the Platforms pairing code")?;
        let i: PairInvite = serde_json::from_str(json).map_err(|e| format!("damaged pairing code: {e}"))?;
        if i.v != 1 { return Err(format!("pairing code version {} not supported", i.v)) }
        Ok(i)
    }
}

/// What a QR invite needs: the relay (always reaches us; iroh then upgrades to direct) and the
/// primary LAN IPv4 (all private IPv4s if unknown). Public, IPv6 and virtual-adapter addresses make
/// the code too dense to scan off a laptop screen; the full list follows in `Ctl::Hello`.
pub fn compact_addr(a: &iroh::EndpointAddr, primary: Option<std::net::Ipv4Addr>) -> iroh::EndpointAddr {
    let keep = |t: &&iroh::TransportAddr| match t {
        iroh::TransportAddr::Relay(_) => true,
        iroh::TransportAddr::Ip(std::net::SocketAddr::V4(v4)) => primary.map_or(v4.ip().is_private(), |p| *v4.ip() == p),
        _ => false,
    };
    iroh::EndpointAddr::from_parts(a.id, a.addrs.iter().filter(keep).cloned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairError { Wrong, Expired, Used }

impl PairError {
    fn as_str(self) -> &'static str { match self { PairError::Wrong => "wrong", PairError::Expired => "expired", PairError::Used => "used" } }
}

/// The code behind one QR invite: random, valid 5 minutes, accepted once.
pub struct PendingCode { code: String, created: Instant, used: bool }

impl PendingCode {
    pub fn new(now: Instant) -> Self {
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).expect("OS random source");
        Self { code: b.iter().map(|x| format!("{x:02x}")).collect(), created: now, used: false }
    }
    pub fn code(&self) -> &str { &self.code }

    /// A wrong guess doesn't burn the code; a correct one does.
    pub fn check(&mut self, code: &str, now: Instant) -> Result<(), PairError> {
        if self.used { return Err(PairError::Used) }
        if now.saturating_duration_since(self.created) > CODE_LIFETIME { return Err(PairError::Expired) }
        if code != self.code { return Err(PairError::Wrong) }
        self.used = true;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Hello { code: String, name: String, kind: PeerKind }
#[derive(Serialize, Deserialize)]
struct Reply { ok: bool, #[serde(default)] name: String, #[serde(default = "android")] kind: PeerKind, #[serde(default)] error: String }
fn android() -> PeerKind { PeerKind::Android }

async fn read_line(r: &mut iroh::endpoint::RecvStream) -> Result<Vec<u8>, String> { read_line_max(r, MAX_LINE).await }

/// Reads one newline-terminated line, refusing lines longer than `max`.
pub(crate) async fn read_line_max(r: &mut iroh::endpoint::RecvStream, max: usize) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        let n = r.read(&mut chunk).await.map_err(|e| e.to_string())?.ok_or("connection closed")?;
        buf.extend_from_slice(&chunk[..n]);
        if let Some(i) = buf.iter().position(|&b| b == b'\n') { buf.truncate(i); return Ok(buf) }
        if buf.len() > max { return Err("message too long".into()) }
    }
}

/// Phone side: proves it saw the code; returns the host as identified by the connection.
pub async fn join(ep: &iroh::Endpoint, invite: &PairInvite, me: &Peer) -> Result<Peer, String> {
    let conn = ep.connect(invite.addr.clone(), PAIR_ALPN).await.map_err(|e| format!("can't reach {}: {e}", invite.name))?;
    let (mut s, mut r) = conn.open_bi().await.map_err(|e| e.to_string())?;
    let hello = serde_json::to_string(&Hello { code: invite.code.clone(), name: me.name.clone(), kind: me.kind }).map_err(|e| e.to_string())?;
    s.write_all(format!("{hello}\n").as_bytes()).await.map_err(|e| e.to_string())?;
    let reply: Reply = serde_json::from_slice(&read_line(&mut r).await?).map_err(|e| e.to_string())?;
    conn.close(0u32.into(), b"");
    if !reply.ok { return Err(format!("pairing refused: {} code", reply.error)) }
    Ok(Peer { id: conn.remote_id().to_string(), name: reply.name, kind: reply.kind })
}

/// Host side: checks the code and answers; returns the phone as identified by the connection.
pub async fn answer(conn: &iroh::endpoint::Connection, pending: &mut PendingCode, me: &Peer, now: Instant) -> Result<Peer, PairError> {
    answer_with(conn, me, |code| pending.check(code, now)).await
}

/// Like `answer`, but `check` is only called once the phone's hello has arrived (within
/// `HELLO_TIMEOUT`), so a silent connection can't hold the pending code.
pub async fn answer_with(conn: &iroh::endpoint::Connection, me: &Peer, check: impl FnOnce(&str) -> Result<(), PairError>) -> Result<Peer, PairError> {
    let read = async {
        let (s, mut r) = conn.accept_bi().await.ok()?;
        let hello: Hello = serde_json::from_slice(&read_line(&mut r).await.ok()?).ok()?;
        Some((s, hello))
    };
    let Ok(Some((mut s, hello))) = tokio::time::timeout(HELLO_TIMEOUT, read).await else { conn.close(1u32.into(), b"no hello"); return Err(PairError::Wrong) };
    let verdict = check(&hello.code);
    let reply = match verdict {
        Ok(()) => Reply { ok: true, name: me.name.clone(), kind: me.kind, error: String::new() },
        Err(e) => Reply { ok: false, name: String::new(), kind: me.kind, error: e.as_str().into() },
    };
    let _ = s.write_all(format!("{}\n", serde_json::to_string(&reply).expect("reply serializes")).as_bytes()).await;
    let _ = s.finish();
    let _ = tokio::time::timeout(HELLO_TIMEOUT, conn.closed()).await;
    verdict.map(|()| Peer { id: conn.remote_id().to_string(), name: hello.name, kind: hello.kind })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test] fn code_works_once() {
        let t = Instant::now(); let mut p = PendingCode::new(t);
        let c = p.code().to_string();
        assert_eq!(p.check(&c, t), Ok(()));
        assert_eq!(p.check(&c, t), Err(PairError::Used));
    }
    #[test] fn code_expires_after_5_min() {
        let t = Instant::now(); let mut p = PendingCode::new(t);
        let c = p.code().to_string();
        assert_eq!(p.check(&c, t + Duration::from_secs(301)), Err(PairError::Expired));
    }
    #[test] fn wrong_code_rejected() {
        let t = Instant::now(); let mut p = PendingCode::new(t);
        assert_eq!(p.check("nope", t), Err(PairError::Wrong));
        let c = p.code().to_string();
        assert_eq!(p.check(&c, t), Ok(()), "a wrong guess doesn't burn the real code");
    }
    #[test] fn codes_are_random() { let t = Instant::now(); assert_ne!(PendingCode::new(t).code(), PendingCode::new(t).code()); }
    #[test] fn qr_round_trip() {
        let addr = iroh::EndpointAddr::from(iroh::SecretKey::generate().public());
        let i = PairInvite { v: 1, name: "Laptop".into(), addr, code: "abc".into() };
        assert_eq!(PairInvite::parse(&i.to_qr_text()).unwrap(), i);
    }
    #[test] fn invite_addr_is_small_enough_to_scan() {
        use iroh::TransportAddr;
        let relay: iroh::RelayUrl = "https://euc1-1.relay.n0.iroh.link./".parse().unwrap();
        let ip = |s: &str| TransportAddr::Ip(s.parse().unwrap());
        let full = iroh::EndpointAddr::from_parts(iroh::SecretKey::generate().public(), [
            TransportAddr::Relay(relay.clone()), ip("93.133.203.10:63866"), ip("172.17.240.1:63866"), ip("192.168.1.3:63866"),
            ip("192.168.24.1:63866"), ip("192.168.112.1:63866"), ip("[2a02:3100:475a:d601:15e3:d48c:12fc:4ed8]:63868"),
        ]);
        let small = compact_addr(&full, Some("192.168.1.3".parse().unwrap()));
        assert_eq!(compact_addr(&full, None).ip_addrs().count(), 4, "unknown primary: every private IPv4");
        assert!(small.addrs.contains(&TransportAddr::Relay(relay)));
        assert!(small.addrs.contains(&ip("192.168.1.3:63866")));
        assert!(!small.addrs.contains(&ip("93.133.203.10:63866")), "public IP: the relay covers it");
        assert!(small.ip_addrs().all(|a| a.is_ipv4()));
        let qr = PairInvite { v: 1, name: "LAPTOP-AMIR".into(), addr: small, code: PendingCode::new(Instant::now()).code().into() }.to_qr_text();
        assert!(qr.len() < 300, "{} chars", qr.len());
    }
    #[test] fn foreign_qr_rejected() {
        assert!(PairInvite::parse("https://example.com").is_err());
        assert!(PairInvite::parse(r#"ctp-pair:{"v":2,"name":"x","addr":{"id":"x","addrs":[]},"code":"c"}"#).is_err());
    }
}
