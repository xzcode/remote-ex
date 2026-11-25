use axum_server::tls_rustls::RustlsConfig;
use remote_ex_rendezvous_client::{Connection,Endpoint};
use remote_ex_rendezvous::protocol::{Client,Server};
use remote_ex_security::DeviceIdentity;
use std::time::Duration;

#[tokio::test]
async fn real_tls_registration_requires_trusted_certificate_hostname_and_server_identity(){
    // Test-only self-signed localhost certificate, never committed to disk.
    let _=rustls::crypto::ring::default_provider().install_default();
    let rcgen::CertifiedKey{cert,key_pair}=rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let config=RustlsConfig::from_pem(cert.pem().into_bytes(),key_pair.serialize_pem().into_bytes()).await.unwrap();
    let mut roots=rustls::RootCertStore::empty();
    roots.add(cert.der().clone()).unwrap();
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port=listener.local_addr().unwrap().port();
    let app=remote_ex_rendezvous::router_with_identity([7;32]).unwrap();
    let handle=axum_server::Handle::new();
    let server=axum_server::from_tcp_rustls(listener,config).unwrap().handle(handle.clone());
    let task=tokio::spawn(async move{server.serve(app.into_make_service()).await.unwrap();});
    tokio::time::timeout(Duration::from_secs(3),handle.listening()).await.unwrap().unwrap();
    let url=format!("wss://localhost:{port}/v1/ws");
    let id=DeviceIdentity::generate();
    assert!(Connection::register_with_roots(Endpoint::secure(&url,[7;32]).unwrap(),&id,rustls::RootCertStore::empty()).await.is_err());
    let bad_name=format!("wss://127.0.0.1:{port}/v1/ws");
    assert!(Connection::register_with_roots(Endpoint::secure(&bad_name,[7;32]).unwrap(),&id,roots.clone()).await.is_err());
    assert!(Connection::register_with_roots(Endpoint::secure(&url,[8;32]).unwrap(),&id,roots.clone()).await.is_err());
    let mut conn=Connection::register_with_roots(Endpoint::secure(&url,[7;32]).unwrap(),&id,roots.clone()).await.unwrap();
    assert_eq!(conn.device_id(),remote_ex_signaling::hex(&id.fingerprint()));
    assert!(Connection::register_with_roots(Endpoint::secure(&url,[7;32]).unwrap(),&id,roots).await.is_err());
    conn.send(Client::Ping).await.unwrap();
    assert!(matches!(conn.receive(Duration::from_secs(3)).await.unwrap(),Server::Pong));
    conn.close().await.unwrap();
    handle.graceful_shutdown(Some(Duration::from_secs(1)));
    tokio::time::timeout(Duration::from_secs(3),task).await.unwrap().unwrap();
}

#[tokio::test]
async fn reusable_client_registers_and_rejects_an_incoming_request(){
    use remote_ex_security::session::VIEW;
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url=format!("ws://{}/v1/ws",listener.local_addr().unwrap());
    let task=tokio::spawn(async move{axum::serve(listener,remote_ex_rendezvous::router()).await.unwrap();});
    let a_id=DeviceIdentity::generate();
    let b_id=DeviceIdentity::generate();
    let mut a=Connection::register(Endpoint::development(&url).unwrap(),&a_id).await.unwrap();
    let mut b=Connection::register(Endpoint::development(&url).unwrap(),&b_id).await.unwrap();
    a.send(Client::Request{target:b.device_id().into(),requested:VIEW,controller_nonce:"01".repeat(32)}).await.unwrap();
    let Server::Requested{session,..}=a.receive(Duration::from_secs(3)).await.unwrap()else{panic!("request")};
    let Server::Incoming{session:incoming,..}=b.receive(Duration::from_secs(3)).await.unwrap()else{panic!("incoming")};
    assert_eq!(incoming,session);
    b.send(Client::Decision{session,grant:None,host_nonce:None}).await.unwrap();
    assert!(matches!(a.receive(Duration::from_secs(3)).await.unwrap(),Server::Decision{grant:None,..}));
    a.close().await.unwrap();
    b.close().await.unwrap();
    task.abort();
}

#[tokio::test]
async fn presence_declines_requests_and_stops_on_local_cancel() {
    use remote_ex_rendezvous_client::{presence,PresenceEvent};
    use remote_ex_security::session::VIEW;
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url=format!("ws://{}/v1/ws",listener.local_addr().unwrap());
    let server=tokio::spawn(async move{axum::serve(listener,remote_ex_rendezvous::router()).await.unwrap();});
    let a_id=DeviceIdentity::generate();
    let b_id=DeviceIdentity::generate();
    let mut a=Connection::register(Endpoint::development(&url).unwrap(),&a_id).await.unwrap();
    let mut b=Connection::register(Endpoint::development(&url).unwrap(),&b_id).await.unwrap();
    let bid=b.device_id().to_owned();
    let(stop,rx)=tokio::sync::watch::channel(false);
    let(events,mut observed)=tokio::sync::mpsc::unbounded_channel();
    let worker=tokio::spawn(async move{
        presence(&mut b,rx,Duration::from_secs(20),|event|{let _=events.send(event);}).await.unwrap();
        b.close().await.unwrap();
    });
    assert_eq!(tokio::time::timeout(Duration::from_secs(3),observed.recv()).await.unwrap(),Some(PresenceEvent::Heartbeat));
    a.send(Client::Request{target:bid,requested:VIEW,controller_nonce:"01".repeat(32)}).await.unwrap();
    assert!(matches!(a.receive(Duration::from_secs(3)).await.unwrap(),Server::Requested{..}));
    assert!(matches!(a.receive(Duration::from_secs(3)).await.unwrap(),Server::Decision{grant:None,..}));
    assert_eq!(tokio::time::timeout(Duration::from_secs(3),observed.recv()).await.unwrap(),Some(PresenceEvent::RequestDeclined));
    stop.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(3),worker).await.unwrap().unwrap();
    a.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn enrollment_rejects_unlisted_identity_and_allows_explicitly_listed(){
    use std::collections::HashSet;
    let permitted=DeviceIdentity::generate();
    let unknown=DeviceIdentity::generate();
    let ids=HashSet::from([remote_ex_signaling::hex(&permitted.fingerprint())]);
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url=format!("ws://{}/v1/ws",listener.local_addr().unwrap());
    let router=remote_ex_rendezvous::router_configured([7;32],None,Some(ids)).unwrap();
    let task=tokio::spawn(async move{axum::serve(listener,router).await.unwrap();});
    assert!(Connection::register(Endpoint::development(&url).unwrap(),&unknown).await.is_err());
    let conn=Connection::register(Endpoint::development(&url).unwrap(),&permitted).await.unwrap();
    conn.close().await.unwrap();task.abort();
}
