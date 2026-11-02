//! bridge-recv <dir>: prints a ticket, then saves every incoming file into <dir>.
#[tokio::main]
async fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let mut rx = bridge_core::transfer::Receiver::start(dir.into()).await.expect("start receiver");
    println!("{}", rx.ticket());
    loop {
        let t = std::time::Instant::now();
        match rx.next().await {
            Ok(r) => eprintln!("received {} bytes ({:.1} MB/s incl. wait)", r.size, r.size as f64 / 1e6 / t.elapsed().as_secs_f64()),
            Err(e) => eprintln!("transfer failed: {e:#}"),
        }
    }
}
