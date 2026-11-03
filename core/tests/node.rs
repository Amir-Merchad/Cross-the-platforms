use std::time::Duration;

use bridge_core::{identity::{Peer, PeerKind}, node::{CTL_ALPN, Event, Node, NodeConfig}};
use tempfile::TempDir;
use tokio::{sync::mpsc::Receiver, time::timeout};

async fn node(name: &str, kind: PeerKind) -> (Node, Receiver<Event>, TempDir) {
    let d = tempfile::tempdir().unwrap();
    let (n, rx) = Node::start(NodeConfig { data_dir: d.path().join("data"), receive_dir: d.path().join("in"), me: Peer { id: String::new(), name: name.into(), kind } }).await.unwrap();
    (n, rx, d)
}

/// Waits for the first event matching `f` (other events are skipped).
async fn wait_for(rx: &mut Receiver<Event>, f: impl Fn(&Event) -> bool) -> Event {
    timeout(Duration::from_secs(15), async { loop { let e = rx.recv().await.expect("node stopped"); if f(&e) { return e } } }).await.expect("timed out")
}

async fn paired() -> ((Node, Receiver<Event>, TempDir), (Node, Receiver<Event>, TempDir)) {
    let (mut a, mut b) = (node("Laptop", PeerKind::Windows).await, node("S24+", PeerKind::Android).await);
    let peer = b.0.join(&a.0.invite()).await.unwrap();
    assert_eq!(peer.id, a.0.id());
    wait_for(&mut a.1, |e| matches!(e, Event::Paired(p) if p.name == "S24+")).await;
    let _ = &mut b;
    (a, b)
}

#[tokio::test]
async fn unpaired_peer_is_rejected() { // Review Focus 1
    let (a, mut rx, _d) = node("Laptop", PeerKind::Windows).await;
    let stranger = iroh::Endpoint::bind(iroh::endpoint::presets::N0).await.unwrap();
    if let Ok(conn) = stranger.connect(a.addr(), CTL_ALPN).await {
        if let Ok((mut s, _r)) = conn.open_bi().await {
            let _ = s.write_all(b"{\"t\":\"Clip\",\"text\":\"hi\"}\n").await;
            let _ = s.finish();
        }
        let _ = timeout(Duration::from_secs(5), conn.closed()).await;
    }
    assert!(timeout(Duration::from_millis(800), rx.recv()).await.is_err(), "no event from an unpaired device");
}

#[tokio::test]
async fn clip_round_trip_between_paired_nodes() {
    let ((a, mut arx, _da), (b, mut brx, _db)) = paired().await;
    b.send_clip(&a.id(), "héllo 👋".into()).await.unwrap();
    let e = wait_for(&mut arx, |e| matches!(e, Event::Clip { .. })).await;
    assert_eq!(e, Event::Clip { from: b.id(), text: "héllo 👋".into() });
    a.send_clip(&b.id(), "back".into()).await.unwrap();
    assert_eq!(wait_for(&mut brx, |e| matches!(e, Event::Clip { .. })).await, Event::Clip { from: a.id(), text: "back".into() });
}

#[tokio::test]
async fn pair_then_restart_keeps_pairing() {
    let ((a, _arx, da), (b, _brx, db)) = paired().await;
    let (aid, bid) = (a.id(), b.id());
    a.shutdown().await; b.shutdown().await;
    let restart = |d: &TempDir, name: &str, kind| NodeConfig { data_dir: d.path().join("data"), receive_dir: d.path().join("in"), me: Peer { id: String::new(), name: name.into(), kind } };
    let (a2, mut arx2) = Node::start(restart(&da, "Laptop", PeerKind::Windows)).await.unwrap();
    let (b2, _brx2) = Node::start(restart(&db, "S24+", PeerKind::Android)).await.unwrap();
    assert_eq!((a2.id(), b2.id()), (aid.clone(), bid.clone()), "same identity after restart");
    assert!(a2.peers().iter().any(|p| p.id == bid));
    b2.send_clip(&aid, "still paired".into()).await.unwrap();
    wait_for(&mut arx2, |e| matches!(e, Event::Clip { text, .. } if text == "still paired")).await;
}

fn write(dir: &std::path::Path, name: &str, n: usize, seed: u8) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, (0..n).map(|i| (i as u8).wrapping_mul(13).wrapping_add(seed)).collect::<Vec<u8>>()).unwrap();
    p
}

async fn received(rx: &mut Receiver<Event>, n: usize) -> Vec<(String, std::path::PathBuf, u64)> {
    let mut v = Vec::new();
    for _ in 0..n { if let Event::Received { from, path, size } = wait_for(rx, |e| matches!(e, Event::Received { .. })).await { v.push((from, path, size)) } }
    v
}

#[tokio::test]
async fn send_files_end_to_end() {
    let ((a, mut arx, _da), (b, _brx, _db)) = paired().await;
    let src = tempfile::tempdir().unwrap();
    let (p1, p2) = (write(src.path(), "notes.pdf", 2_000_000, 1), write(src.path(), "clip.mp4", 3_000_000, 2));
    assert_eq!(b.send_files(&a.id(), vec![p1.clone(), p2.clone()]).await.unwrap(), 5_000_000);
    let got = received(&mut arx, 2).await;
    assert!(got.iter().all(|(from, _, _)| *from == b.id()));
    assert_eq!(got.iter().map(|g| g.2).collect::<Vec<_>>(), vec![2_000_000, 3_000_000]);
    assert_eq!(std::fs::read(&got[0].1).unwrap(), std::fs::read(&p1).unwrap());
    assert_eq!(std::fs::read(&got[1].1).unwrap(), std::fs::read(&p2).unwrap());
}

#[tokio::test]
async fn falls_back_to_iroh_when_tcp_unreachable() { // Review Focus 4
    let ((a, mut arx, _da), (b, _brx, _db)) = paired().await;
    a.test_bulk_addrs(Some(vec!["10.255.255.1:9".parse().unwrap()]));
    let src = tempfile::tempdir().unwrap();
    let p = write(src.path(), "slides.pdf", 1_000_000, 3);
    let t = std::time::Instant::now();
    b.send_files(&a.id(), vec![p.clone()]).await.unwrap();
    let got = received(&mut arx, 1).await;
    assert!(t.elapsed() < Duration::from_secs(8), "took {:?}", t.elapsed());
    assert_eq!(std::fs::read(&got[0].1).unwrap(), std::fs::read(&p).unwrap());
}

#[tokio::test]
async fn send_to_unpaired_peer_fails_cleanly() {
    let (a, _arx, _da) = node("Laptop", PeerKind::Windows).await;
    let (c, _crx, _dc) = node("Stranger", PeerKind::Android).await;
    let src = tempfile::tempdir().unwrap();
    let p = write(src.path(), "x.bin", 10, 4);
    assert!(c.send_files(&a.id(), vec![p]).await.is_err());
}
