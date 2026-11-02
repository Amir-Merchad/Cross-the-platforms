//! bridge-send <ticket> <file>: sends one file and prints MB/s.
#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (ticket, file) = (a.first().expect("ticket"), a.get(1).expect("file"));
    let t = std::time::Instant::now();
    let n = bridge_core::transfer::send_file(ticket, file.as_ref()).await.expect("send failed");
    let s = t.elapsed().as_secs_f64();
    println!("sent {n} bytes in {s:.2}s = {:.1} MB/s", n as f64 / 1e6 / s);
}
