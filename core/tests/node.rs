use std::time::Duration;

use bridge_core::{identity::{Peer, PeerKind, load_or_create_key}, node::{CTL_ALPN, Ctl, Event, Node, NodeConfig, SendOutcome}, outbox::Outbox};
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
async fn reachable_lists_peers_recently_heard() {
    let ((a, mut arx, _da), (b, _brx, _db)) = paired().await;
    b.send_clip(&a.id(), "hi".into()).await.unwrap();
    wait_for(&mut arx, |e| matches!(e, Event::Clip { .. })).await;
    assert_eq!(a.reachable(Duration::from_secs(60)), vec![b.id()]);
    assert!(a.reachable(Duration::ZERO).is_empty());
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
    let (pc2, _pcrx2) = restart(&dp, "Laptop", PeerKind::Windows).await;
    assert_eq!(pc2.waiting(), vec![(ipad_id.clone(), 1)], "the restarted PC still knows what waits");
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

#[tokio::test]
async fn started_names_the_files_on_both_sides() {
    let ((a, mut arx, _da), (b, mut brx, _db)) = paired().await;
    let src = tempfile::tempdir().unwrap();
    let (f1, f2) = (write(src.path(), "one.bin", 300, 1), write(src.path(), "two.bin", 500, 2));
    a.send_files(&b.id(), vec![f1.clone(), f2.clone()]).await.unwrap();
    let Event::Started { id, peer, outgoing, name, files, total, paths } = wait_for(&mut arx, |e| matches!(e, Event::Started { .. })).await else { unreachable!() };
    assert_eq!((peer, outgoing, name.as_str(), files, total, paths), (b.id(), true, "one.bin", 2, 800, vec![f1, f2]));
    let rid = id;
    let Event::Started { id, peer, outgoing, name, files, total, paths } = wait_for(&mut brx, |e| matches!(e, Event::Started { .. })).await else { unreachable!() };
    assert_eq!((id, peer, outgoing, name.as_str(), files, total, paths.len()), (rid, a.id(), false, "one.bin", 2, 800, 0));
}

#[tokio::test]
async fn batch_stays_queued_when_the_stream_is_cut_mid_transfer() { // final review I1
    let ((pc, mut pcrx, dp), (ipad, _irx, di)) = paired_with(PeerKind::Ipad).await;
    let ipad_id = ipad.id();
    ipad.shutdown().await;
    let src = tempfile::tempdir().unwrap();
    let f = write(src.path(), "big.bin", 1_000_000, 12);
    assert!(matches!(pc.send_or_queue(&ipad_id, vec![f.clone()]).await.unwrap(), SendOutcome::Queued));
    // The iPad comes back, accepts the offer, then dies mid-receive (its receive dir can't be created).
    let blocker = di.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    let cfg = NodeConfig { data_dir: di.path().join("data"), receive_dir: blocker.join("in"), me: Peer { id: String::new(), name: "S24+".into(), kind: PeerKind::Ipad } };
    let (ipad2, _irx2) = Node::start(cfg).await.unwrap();
    // The flush ran, its attempt stopped, and the batch stayed queued.
    let Event::Started { id, .. } = wait_for(&mut pcrx, |e| matches!(e, Event::Started { .. })).await else { unreachable!() };
    wait_for(&mut pcrx, |e| *e == Event::Requeued { id }).await;
    assert!(timeout(Duration::from_millis(1500), async { loop { if let Some(Event::Failed { .. }) = pcrx.recv().await { return } } }).await.is_err(), "a cut stream is not a failure");
    assert_eq!(Outbox::load(&dp.path().join("data").join("outbox.json")).pending(&ipad_id), 1, "still on disk");
    ipad2.shutdown().await;
    let (_ipad3, mut irx3) = restart(&di, "S24+", PeerKind::Ipad).await;
    let got = received(&mut irx3, 1).await;
    assert_eq!(std::fs::read(&got[0].1).unwrap(), std::fs::read(&f).unwrap());
}

#[tokio::test]
async fn empty_file_send_ends_with_a_final_progress() { // base UI final review 1
    let ((a, mut arx, _da), (b, mut brx, _db)) = paired().await;
    let src = tempfile::tempdir().unwrap();
    let p = write(src.path(), "empty.txt", 0, 0);
    assert_eq!(b.send_files(&a.id(), vec![p]).await.unwrap(), 0);
    wait_for(&mut brx, |e| matches!(e, Event::Progress { done: 0, total: 0, .. })).await;
    received(&mut arx, 1).await;
}

/// Stands in for a paired device: takes a whole file of `size` bytes over the iroh fallback, then reports a bad checksum.
async fn refusing_receiver(key: iroh::SecretKey, sender: iroh::EndpointAddr, size: usize) -> iroh::Endpoint {
    let ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).secret_key(key).alpns(vec![CTL_ALPN.to_vec(), b"ctp/bulk/1".to_vec()]).bind().await.unwrap();
    // Say where we are, as a restarted device would.
    let c = ep.connect(sender, CTL_ALPN).await.unwrap();
    let (mut s, mut r) = c.open_bi().await.unwrap();
    s.write_all(format!("{}\n", serde_json::to_string(&Ctl::Hello { addr: ep.addr() }).unwrap()).as_bytes()).await.unwrap();
    s.finish().unwrap();
    r.read_to_end(1024).await.unwrap();
    let listening = ep.clone();
    tokio::spawn(async move {
        while let Some(incoming) = listening.accept().await {
            tokio::spawn(async move {
                let conn = incoming.await.unwrap();
                let (mut s, mut r) = conn.accept_bi().await.unwrap();
                if conn.alpn().to_vec() == CTL_ALPN {
                    let lines = r.read_to_end(1 << 20).await.unwrap();
                    for l in lines.split(|&b| b == b'\n') {
                        if let Ok(Ctl::Offer { id, .. }) = serde_json::from_slice(l) {
                            // No TCP address, so the sender goes straight to the iroh fallback.
                            let accept = Ctl::Accept { id, tcp: Vec::new(), cert_sha256: String::new(), token: "0".repeat(32) };
                            s.write_all(format!("{}\n", serde_json::to_string(&accept).unwrap()).as_bytes()).await.unwrap();
                        }
                    }
                } else {
                    // Offer id, token, size, the file, its hash.
                    r.read_exact(&mut vec![0u8; 8 + 32 + 8 + size + 32]).await.unwrap();
                    s.write_all(&[0]).await.unwrap();
                }
                let _ = s.finish();
                conn.closed().await;
            });
        }
    });
    ep
}

#[tokio::test]
async fn receiver_failure_after_the_last_byte_is_failed_not_sent() { // base UI final review 1
    let ((a, _arx, da), (b, mut brx, _db)) = paired().await;
    let id = a.id();
    a.shutdown().await;
    let key = load_or_create_key(&da.path().join("data").join("identity.key")).unwrap();
    let _ep = refusing_receiver(key, b.addr(), 3000).await;
    let src = tempfile::tempdir().unwrap();
    let p = write(src.path(), "notes.pdf", 3000, 4);
    assert!(b.send_files(&id, vec![p]).await.unwrap_err().contains("checksum"));
    let e = wait_for(&mut brx, |e| matches!(e, Event::Failed { .. }) || matches!(e, Event::Progress { done, total, .. } if done == total)).await;
    assert!(matches!(e, Event::Failed { .. }), "{e:?}");
}

/// Roadmap step 8: a phone and an iPad pair with each other and share the clipboard with no PC in between.
#[tokio::test]
async fn phone_and_ipad_share_the_clipboard_directly() {
    let (phone, mut prx, _pd) = node("S24+", PeerKind::Android).await;
    let (ipad, mut irx, _id) = node("iPad Air", PeerKind::Ipad).await;
    // The phone shows the code, the iPad scans it: no Windows device involved.
    let peer = ipad.join(&phone.invite()).await.unwrap();
    assert_eq!(peer.id, phone.id());
    wait_for(&mut prx, |e| matches!(e, Event::Paired(p) if p.name == "iPad Air")).await;

    assert_eq!(phone.share_clip("chapter 3 summary".into()).await, 1);
    wait_for(&mut irx, |e| matches!(e, Event::Clip { text, .. } if text == "chapter 3 summary")).await;
    assert_eq!(phone.share_clip("chapter 3 summary".into()).await, 0, "the same copy again is not resent");
    // The iPad writing it to its clipboard shows up as a local change: that echo must not come back.
    assert_eq!(ipad.share_clip("chapter 3 summary".into()).await, 0);
    // A real new copy on the iPad does go to the phone.
    assert_eq!(ipad.share_clip("formula".into()).await, 1);
    wait_for(&mut prx, |e| matches!(e, Event::Clip { text, .. } if text == "formula")).await;
}

#[tokio::test]
async fn unknown_control_message_is_ignored() { // Review Focus 2
    assert_eq!(serde_json::from_str::<Ctl>(r#"{"t":"FromTheFuture","x":1}"#).unwrap(), Ctl::Unknown);
    let ((a, mut arx, _da), (_b, _brx, db)) = paired().await;
    // Speak as the paired phone: an unknown message first, then a clip on the same stream.
    let key = load_or_create_key(&db.path().join("data").join("identity.key")).unwrap();
    let ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).secret_key(key).alpns(vec![CTL_ALPN.to_vec()]).bind().await.unwrap();
    let c = ep.connect(a.addr(), CTL_ALPN).await.unwrap();
    let (mut s, mut r) = c.open_bi().await.unwrap();
    s.write_all(b"{\"t\":\"FromTheFuture\",\"x\":1}\n{\"t\":\"Clip\",\"text\":\"after\"}\n").await.unwrap();
    s.finish().unwrap();
    assert!(r.read_to_end(1024).await.unwrap().is_empty(), "no reply to an unknown message");
    wait_for(&mut arx, |e| matches!(e, Event::Clip { text, .. } if text == "after")).await;
}

#[tokio::test]
async fn note_reaches_the_pc() {
    let ((a, mut arx, _da), (b, _brx, _db)) = paired().await;
    b.send_note(&a.id(), "remember the 3pm lab".into()).await.unwrap();
    let e = wait_for(&mut arx, |e| matches!(e, Event::Note { .. })).await;
    assert_eq!(e, Event::Note { from: b.id(), text: "remember the 3pm lab".into() });
    // An empty note is not a note.
    b.send_note(&a.id(), "  \n".into()).await.unwrap();
    b.send_note(&a.id(), "next".into()).await.unwrap();
    assert_eq!(wait_for(&mut arx, |e| matches!(e, Event::Note { .. })).await, Event::Note { from: b.id(), text: "next".into() });
}
