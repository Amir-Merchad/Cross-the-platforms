use std::path::PathBuf;

use bridge_core::{bulk::{BulkListener, send}, node::FileMeta};
use tempfile::tempdir;

fn data(n: usize, seed: u8) -> Vec<u8> { (0..n).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect() }
fn meta(name: &str, size: usize) -> FileMeta { FileMeta { name: name.into(), size: size as u64 } }
fn files_in(d: &std::path::Path) -> Vec<String> { let mut v: Vec<_> = std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect(); v.sort(); v }

#[tokio::test]
async fn two_files_round_trip() {
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let (a, b) = (src.path().join("a.pdf"), src.path().join("b.mp4"));
    std::fs::write(&a, data(3_000_000, 1)).unwrap();
    std::fs::write(&b, data(5_000_000, 2)).unwrap();
    let l = BulkListener::bind().await.unwrap();
    let (addrs, cert, tok) = (l.addrs(), l.cert_sha256(), l.token());
    let dir = dst.path().to_path_buf();
    let rx = tokio::spawn(async move { l.receive(&[meta("a.pdf", 3_000_000), meta("b.mp4", 5_000_000)], &dir, |_, _| {}).await });
    let sent = send(&addrs, &cert, &tok, &[a.clone(), b.clone()], |_, _| {}).await.unwrap();
    let got = rx.await.unwrap().unwrap();
    assert_eq!(sent, 8_000_000);
    assert_eq!(got, vec![dst.path().join("a.pdf"), dst.path().join("b.mp4")]);
    assert_eq!(std::fs::read(&got[1]).unwrap(), std::fs::read(&b).unwrap());
}

#[tokio::test]
async fn wrong_fingerprint_is_refused() {
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let f = src.path().join("x.bin"); std::fs::write(&f, data(1000, 3)).unwrap();
    let l = BulkListener::bind().await.unwrap();
    let (addrs, tok) = (l.addrs(), l.token());
    let dir = dst.path().to_path_buf();
    let rx = tokio::spawn(async move { l.receive(&[meta("x.bin", 1000)], &dir, |_, _| {}).await });
    assert!(send(&addrs, &"00".repeat(32), &tok, &[f], |_, _| {}).await.is_err());
    assert!(rx.await.unwrap().is_err());
    assert!(files_in(dst.path()).is_empty());
}

#[tokio::test]
async fn cut_off_leaves_no_file() { // Review Focus 3
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let f = src.path().join("big.mp4"); std::fs::write(&f, data(20 << 20, 4)).unwrap();
    let l = BulkListener::bind().await.unwrap();
    let (addrs, cert, tok) = (l.addrs(), l.cert_sha256(), l.token());
    let dir = dst.path().to_path_buf();
    let rx = tokio::spawn(async move { l.receive(&[meta("big.mp4", 20 << 20)], &dir, |_, _| {}).await });
    let (tx, mut started) = tokio::sync::mpsc::unbounded_channel();
    let sender = tokio::spawn(async move { send(&addrs, &cert, &tok, &[f], move |done, _| { if done >= 1 << 20 { let _ = tx.send(()); } }).await });
    started.recv().await;
    sender.abort();
    assert!(rx.await.unwrap().is_err());
    assert!(files_in(dst.path()).is_empty(), "no final file and no .part: {:?}", files_in(dst.path()));
}

#[tokio::test]
async fn duplicate_names_both_kept() { // Review Focus 5
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let (a1, a2) = (src.path().join("1").join("a.pdf"), src.path().join("2").join("a.pdf"));
    for (p, s) in [(&a1, 5u8), (&a2, 6)] { std::fs::create_dir_all(p.parent().unwrap()).unwrap(); std::fs::write(p, data(1000, s)).unwrap(); }
    let l = BulkListener::bind().await.unwrap();
    let (addrs, cert, tok) = (l.addrs(), l.cert_sha256(), l.token());
    let dir = dst.path().to_path_buf();
    let rx = tokio::spawn(async move { l.receive(&[meta("a.pdf", 1000), meta("a.pdf", 1000)], &dir, |_, _| {}).await });
    send(&addrs, &cert, &tok, &[a1, a2], |_, _| {}).await.unwrap();
    rx.await.unwrap().unwrap();
    assert_eq!(files_in(dst.path()), vec!["a (1).pdf".to_string(), "a.pdf".to_string()]);
}

#[tokio::test]
async fn hostile_name_stays_inside() {
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let f = src.path().join("x"); std::fs::write(&f, data(10, 7)).unwrap();
    let l = BulkListener::bind().await.unwrap();
    let (addrs, cert, tok) = (l.addrs(), l.cert_sha256(), l.token());
    let dir = dst.path().to_path_buf();
    let rx = tokio::spawn(async move { l.receive(&[meta("../../evil.txt", 10)], &dir, |_, _| {}).await });
    send(&addrs, &cert, &tok, &[f], |_, _| {}).await.unwrap();
    assert_eq!(rx.await.unwrap().unwrap(), vec![dst.path().join("evil.txt")]);
}

#[tokio::test] #[ignore]
async fn loopback_throughput() {
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let f = src.path().join("1g.bin"); std::fs::write(&f, data(1 << 30, 8)).unwrap();
    let l = BulkListener::bind().await.unwrap();
    let (addrs, cert, tok) = (l.addrs(), l.cert_sha256(), l.token());
    let dir: PathBuf = dst.path().to_path_buf();
    let rx = tokio::spawn(async move { l.receive(&[meta("1g.bin", 1 << 30)], &dir, |_, _| {}).await });
    let t = std::time::Instant::now();
    send(&addrs, &cert, &tok, &[f], |_, _| {}).await.unwrap();
    rx.await.unwrap().unwrap();
    println!("THROUGHPUT {:.1} MB/s", (1u64 << 30) as f64 / 1e6 / t.elapsed().as_secs_f64());
}

#[tokio::test]
async fn wrong_token_is_refused() { // a LAN stranger who knows the port can't inject files
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let f = src.path().join("x.bin"); std::fs::write(&f, data(1000, 9)).unwrap();
    let l = BulkListener::bind().await.unwrap();
    let (addrs, cert) = (l.addrs(), l.cert_sha256());
    let dir = dst.path().to_path_buf();
    let rx = tokio::spawn(async move { l.receive(&[meta("x.bin", 1000)], &dir, |_, _| {}).await });
    assert!(send(&addrs, &cert, &"0".repeat(32), &[f], |_, _| {}).await.is_err());
    assert!(rx.await.unwrap().is_err());
    assert!(files_in(dst.path()).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_transfers_with_the_same_name_never_share_a_file() { // final review I2
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let (f1, f2) = (src.path().join("1.bin"), src.path().join("2.bin"));
    std::fs::write(&f1, data(8 << 20, 11)).unwrap();
    std::fs::write(&f2, data(8 << 20, 22)).unwrap();
    let (l1, l2) = (BulkListener::bind().await.unwrap(), BulkListener::bind().await.unwrap());
    let (a1, c1, t1) = (l1.addrs(), l1.cert_sha256(), l1.token());
    let (a2, c2, t2) = (l2.addrs(), l2.cert_sha256(), l2.token());
    let (d1, d2) = (dst.path().to_path_buf(), dst.path().to_path_buf());
    let r1 = tokio::spawn(async move { l1.receive(&[meta("IMG_0001.jpg", 8 << 20)], &d1, |_, _| {}).await });
    let r2 = tokio::spawn(async move { l2.receive(&[meta("IMG_0001.jpg", 8 << 20)], &d2, |_, _| {}).await });
    let (v1, v2) = ([f1.clone()], [f2.clone()]);
    let (s1, s2) = tokio::join!(send(&a1, &c1, &t1, &v1, |_, _| {}), send(&a2, &c2, &t2, &v2, |_, _| {}));
    s1.unwrap(); s2.unwrap();
    let (p1, p2) = (r1.await.unwrap().unwrap().remove(0), r2.await.unwrap().unwrap().remove(0));
    assert_ne!(p1, p2);
    assert_eq!(files_in(dst.path()), vec!["IMG_0001 (1).jpg".to_string(), "IMG_0001.jpg".to_string()]);
    assert_eq!(std::fs::read(&p1).unwrap(), std::fs::read(&f1).unwrap(), "first transfer intact");
    assert_eq!(std::fs::read(&p2).unwrap(), std::fs::read(&f2).unwrap(), "second transfer intact");
}
