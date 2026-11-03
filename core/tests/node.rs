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
