use std::time::Instant;

use bridge_core::{identity::{Peer, PeerKind}, pairing::{PAIR_ALPN, PairInvite, PendingCode, answer, join}};
use iroh::{Endpoint, endpoint::presets};

#[tokio::test]
async fn pair_two_endpoints_locally() {
    let host = Endpoint::builder(presets::N0).alpns(vec![PAIR_ALPN.to_vec()]).bind().await.unwrap();
    let phone = Endpoint::bind(presets::N0).await.unwrap();
    let mut pending = PendingCode::new(Instant::now());
    let invite = PairInvite { v: 1, name: "Laptop".into(), addr: host.addr(), code: pending.code().to_string() };
    let host_me = Peer { id: String::new(), name: "Laptop".into(), kind: PeerKind::Windows };
    let phone_me = Peer { id: String::new(), name: "S24+".into(), kind: PeerKind::Android };

    let h = host.clone();
    let server = tokio::spawn(async move {
        let conn = h.accept().await.unwrap().await.unwrap();
        answer(&conn, &mut pending, &host_me, Instant::now()).await
    });
    let got_host = join(&phone, &invite, &phone_me).await.unwrap();
    let got_phone = server.await.unwrap().unwrap();

    assert_eq!(got_host.id, host.id().to_string());
    assert_eq!(got_host.name, "Laptop");
    assert_eq!(got_phone.id, phone.id().to_string(), "id comes from the connection");
    assert_eq!(got_phone.kind, PeerKind::Android);
}

#[tokio::test]
async fn wrong_code_is_refused_by_the_host() {
    let host = Endpoint::builder(presets::N0).alpns(vec![PAIR_ALPN.to_vec()]).bind().await.unwrap();
    let phone = Endpoint::bind(presets::N0).await.unwrap();
    let mut pending = PendingCode::new(Instant::now());
    let invite = PairInvite { v: 1, name: "Laptop".into(), addr: host.addr(), code: "guess".into() };
    let me = Peer { id: String::new(), name: "x".into(), kind: PeerKind::Android };
    let h = host.clone();
    let host_me = me.clone();
    let server = tokio::spawn(async move {
        let conn = h.accept().await.unwrap().await.unwrap();
        answer(&conn, &mut pending, &host_me, Instant::now()).await
    });
    assert!(join(&phone, &invite, &me).await.unwrap_err().contains("wrong"));
    assert!(server.await.unwrap().is_err());
}
