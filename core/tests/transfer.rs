use bridge_core::transfer::{ALPN, Receiver, send_file};
use tempfile::tempdir;

fn random_bytes(n: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E3779B97F4A7C15;
    (0..n).map(|_| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x as u8 }).collect()
}

#[tokio::test]
async fn round_trip_identical_bytes() {
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let data = random_bytes(5 * 1024 * 1024);
    let path = src.path().join("notes.pdf");
    std::fs::write(&path, &data).unwrap();

    let mut rx = Receiver::start(dst.path().to_path_buf()).await.unwrap();
    let ticket = rx.ticket();
    let send = tokio::spawn(async move { send_file(&ticket, &path).await });
    let got = rx.next().await.unwrap();

    assert_eq!(send.await.unwrap().unwrap(), data.len() as u64);
    assert_eq!(got.size, data.len() as u64);
    assert_eq!(got.path, dst.path().join("notes.pdf"));
    assert_eq!(std::fs::read(&got.path).unwrap(), data);
}

#[tokio::test]
async fn hostile_name_stays_in_receive_dir() {
    let (src, dst) = (tempdir().unwrap(), tempdir().unwrap());
    let path = src.path().join("x.txt");
    std::fs::write(&path, b"hi").unwrap();
    let mut rx = Receiver::start(dst.path().to_path_buf()).await.unwrap();
    // Raw sender with a traversal name, bypassing send_file's own naming.
    let ticket = rx.ticket();
    let send = tokio::spawn(async move { raw_send(&ticket, r#"{"v":1,"name":"../../evil.txt","size":2}"#, b"hi").await });
    let got = rx.next().await.unwrap();
    send.await.unwrap();
    assert_eq!(got.path, dst.path().join("evil.txt"));
}

#[tokio::test]
async fn cut_off_transfer_leaves_no_file() {
    let dst = tempdir().unwrap();
    let mut rx = Receiver::start(dst.path().to_path_buf()).await.unwrap();
    let ticket = rx.ticket();
    // Claims 1000 bytes, sends 10, then finishes the stream.
    let send = tokio::spawn(async move { raw_send(&ticket, r#"{"v":1,"name":"a.pdf","size":1000}"#, &[7u8; 10]).await });
    assert!(rx.next().await.is_err());
    send.await.unwrap();
    assert_eq!(std::fs::read_dir(dst.path()).unwrap().count(), 0, "no .part or final file left");
}

async fn raw_send(ticket: &str, header: &str, body: &[u8]) {
    let addr: iroh::EndpointAddr = serde_json::from_str(ticket).unwrap();
    let ep = iroh::Endpoint::bind(iroh::endpoint::presets::N0).await.unwrap();
    let conn = ep.connect(addr, ALPN).await.unwrap();
    let (mut s, mut r) = conn.open_bi().await.unwrap();
    s.write_all(header.as_bytes()).await.unwrap();
    s.write_all(b"\n").await.unwrap();
    s.write_all(body).await.unwrap();
    s.finish().unwrap();
    let _ = r.read_to_end(128).await;
    conn.close(0u32.into(), b"");
    ep.close().await;
}
