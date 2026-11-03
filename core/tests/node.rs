use std::time::Duration;

use bridge_core::{identity::{Peer, PeerKind}, node::{CTL_ALPN, Event, Node, NodeConfig, SendOutcome}, outbox::Outbox};
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

async fn paired() -> ((Node, Receiver<Event>, TempDir), (Node, Receiver<Event>, TempDir)) { paired_with(PeerKind::Android).await }

async fn paired_with(kind: PeerKind) -> ((Node, Receiver<Event>, TempDir), (Node, Receiver<Event>, TempDir)) {
    let (mut a, mut b) = (node("Laptop", PeerKind::Windows).await, node("S24+", kind).await);
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

#[tokio::test]
async fn silent_pairing_connection_does_not_block_the_real_phone() { // final review I1
    let (mut a, _arx, _da) = node("Laptop", PeerKind::Windows).await;
    let (b, _brx, _db) = node("S24+", PeerKind::Android).await;
    let invite = a.invite();
    // Someone who saw an old QR: opens a pairing stream and never sends a line.
    let stranger = iroh::Endpoint::bind(iroh::endpoint::presets::N0).await.unwrap();
    let conn = stranger.connect(a.addr(), bridge_core::pairing::PAIR_ALPN).await.unwrap();
    let (mut s, _r) = conn.open_bi().await.unwrap();
    s.write_all(b"{").await.unwrap(); // makes the stream visible to the hub, never finishes the line
    tokio::time::sleep(Duration::from_millis(500)).await;
    let peer = timeout(Duration::from_secs(8), b.join(&invite)).await.expect("join hung").expect("real phone must still pair");
    assert_eq!(peer.id, a.id());
    let _ = &mut a;
    drop(conn);
}

#[tokio::test]
async fn failed_send_is_reported_to_the_sender_window() { // final review I3
    let (a, _arx, _da) = node("Laptop", PeerKind::Windows).await;
    let (c, mut crx, _dc) = node("Stranger", PeerKind::Android).await;
    let src = tempfile::tempdir().unwrap();
    let p = write(src.path(), "x.bin", 10, 5);
    assert!(c.send_files(&a.id(), vec![p]).await.is_err());
    let e = wait_for(&mut crx, |e| matches!(e, Event::Failed { .. })).await;
    assert!(matches!(e, Event::Failed { reason, .. } if reason.contains("not paired")));
    assert!(c.send_files(&a.id(), vec![src.path().join("missing.pdf")]).await.is_err());
    wait_for(&mut crx, |e| matches!(e, Event::Failed { .. })).await;
}

async fn restart(d: &TempDir, name: &str, kind: PeerKind) -> (Node, Receiver<Event>) {
    Node::start(NodeConfig { data_dir: d.path().join("data"), receive_dir: d.path().join("in"), me: Peer { id: String::new(), name: name.into(), kind } }).await.unwrap()
}

#[tokio::test]
async fn send_to_closed_ipad_queues_and_arrives_on_next_start() { // Review Focus 1
    let ((pc, mut pcrx, _dp), (ipad, _irx, di)) = paired_with(PeerKind::Ipad).await;
    let ipad_id = ipad.id();
    ipad.shutdown().await;
    let src = tempfile::tempdir().unwrap();
    let f = write(src.path(), "notes.pdf", 500_000, 6);
    assert!(matches!(pc.send_or_queue(&ipad_id, vec![f.clone()]).await.unwrap(), SendOutcome::Queued));
    wait_for(&mut pcrx, |e| matches!(e, Event::Queued { files: 1, .. })).await;
    let (_ipad2, mut irx2) = restart(&di, "S24+", PeerKind::Ipad).await;
    let got = received(&mut irx2, 1).await;
    assert_eq!(std::fs::read(&got[0].1).unwrap(), std::fs::read(&f).unwrap());
    wait_for(&mut pcrx, |e| matches!(e, Event::Queued { files: 0, .. })).await;
}

#[tokio::test]
async fn queue_survives_a_pc_restart() { // Review Focus 1
    let ((pc, mut pcrx, dp), (ipad, _irx, di)) = paired_with(PeerKind::Ipad).await;
    let ipad_id = ipad.id();
    ipad.shutdown().await;
    let src = tempfile::tempdir().unwrap();
    let f = write(src.path(), "notes.pdf", 500_000, 7);
    assert!(matches!(pc.send_or_queue(&ipad_id, vec![f.clone()]).await.unwrap(), SendOutcome::Queued));
    wait_for(&mut pcrx, |e| matches!(e, Event::Queued { .. })).await;
    pc.shutdown().await;
    let (_pc2, _pcrx2) = restart(&dp, "Laptop", PeerKind::Windows).await;
    let (_ipad2, mut irx2) = restart(&di, "S24+", PeerKind::Ipad).await;
    let got = received(&mut irx2, 1).await;
    assert_eq!(std::fs::read(&got[0].1).unwrap(), std::fs::read(&f).unwrap());
}

#[tokio::test]
async fn missing_queued_file_fails_alone() { // Review Focus 2
    let ((pc, mut pcrx, dp), (ipad, _irx, di)) = paired_with(PeerKind::Ipad).await;
    let ipad_id = ipad.id();
    ipad.shutdown().await;
    let src = tempfile::tempdir().unwrap();
    let (a, b) = (write(src.path(), "a.pdf", 1000, 8), write(src.path(), "b.pdf", 2000, 9));
    for f in [&a, &b] { assert!(matches!(pc.send_or_queue(&ipad_id, vec![f.clone()]).await.unwrap(), SendOutcome::Queued)); }
    std::fs::remove_file(&a).unwrap();
    let (_ipad2, mut irx2) = restart(&di, "S24+", PeerKind::Ipad).await;
    wait_for(&mut pcrx, |e| matches!(e, Event::Failed { .. })).await;
    let got = received(&mut irx2, 1).await;
    assert_eq!(std::fs::read(&got[0].1).unwrap(), std::fs::read(&b).unwrap());
    wait_for(&mut pcrx, |e| matches!(e, Event::Queued { files: 0, .. })).await;
    assert_eq!(Outbox::load(&dp.path().join("data").join("outbox.json")).pending(&ipad_id), 0);
}

#[tokio::test]
async fn closed_android_peer_is_not_queued() {
    let ((pc, _pcrx, dp), (phone, _prx, _dd)) = paired().await;
    let phone_id = phone.id();
    phone.shutdown().await;
    let src = tempfile::tempdir().unwrap();
    assert!(pc.send_or_queue(&phone_id, vec![write(src.path(), "x.bin", 10, 10)]).await.is_err());
    assert_eq!(Outbox::load(&dp.path().join("data").join("outbox.json")).pending(&phone_id), 0);
}

#[tokio::test]
async fn batch_stays_queued_when_the_ipad_goes_away_mid_flush() { // Review Focus 1
    let ((pc, mut pcrx, dp), (ipad, _irx, di)) = paired_with(PeerKind::Ipad).await;
    let ipad_id = ipad.id();
    ipad.shutdown().await;
    let src = tempfile::tempdir().unwrap();
    let f = write(src.path(), "notes.pdf", 1000, 11);
    assert!(matches!(pc.send_or_queue(&ipad_id, vec![f.clone()]).await.unwrap(), SendOutcome::Queued));
    pc.flush_outbox(&ipad_id).await; // the iPad said Hello, then vanished
    assert_eq!(Outbox::load(&dp.path().join("data").join("outbox.json")).pending(&ipad_id), 1, "still on disk");
    assert!(timeout(Duration::from_millis(300), async { loop { if let Some(Event::Failed { .. }) = pcrx.recv().await { return } } }).await.is_err(), "going away is not a failure");
    let (_ipad2, mut irx2) = restart(&di, "S24+", PeerKind::Ipad).await;
    let got = received(&mut irx2, 1).await;
    assert_eq!(std::fs::read(&got[0].1).unwrap(), std::fs::read(&f).unwrap());
}
