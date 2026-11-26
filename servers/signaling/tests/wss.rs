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
    assert!(Connection::preflight_with_roots(Endpoint::secure(&url,[7;32]).unwrap(),roots.clone()).await.is_ok());
    assert!(Connection::preflight_with_roots(Endpoint::secure(&url,[8;32]).unwrap(),roots.clone()).await.is_err());
    assert!(Connection::preflight_with_roots(Endpoint::secure(&bad_name,[7;32]).unwrap(),roots.clone()).await.is_err());
    assert!(Connection::preflight_with_roots(Endpoint::secure(&url,[7;32]).unwrap(),rustls::RootCertStore::empty()).await.is_err());
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

#[tokio::test]
async fn presence_reconnects_with_same_identity_after_transport_loss(){
    use futures_util::{SinkExt,StreamExt};
    use remote_ex_rendezvous_client::{online_with_retry,PresenceLifecycle};
    use remote_ex_rendezvous::protocol::{ClientFrame,ServerFrame,VERSION};
    use remote_ex_security::verify_registration;
    use remote_ex_signaling::{decode_hex,hex};
    use tokio_tungstenite::tungstenite::Message;
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url=format!("ws://{}/v1/ws",listener.local_addr().unwrap());
    let id=DeviceIdentity::generate();let expected=id.public_key();
    let device_id=hex(&id.fingerprint());let expected_id=device_id.clone();
    let fixture=tokio::spawn(async move{
        for attempt in 1..=2u8{
            let(stream,_)=listener.accept().await.unwrap();
            let mut ws=tokio_tungstenite::accept_async(stream).await.unwrap();
            let server=[7;32];let nonce=[attempt;32];
            let challenge=ServerFrame{version:VERSION,message:Server::Challenge{server:hex(&server),nonce:hex(&nonce)}};
            ws.send(Message::Text(serde_json::to_string(&challenge).unwrap().into())).await.unwrap();
            let m=ws.next().await.unwrap().unwrap();
            let c:ClientFrame=serde_json::from_str(m.to_text().unwrap()).unwrap();
            let Client::Register{public_key,signature}=c.message else{panic!("register")};
            let key=decode_hex::<32>(&public_key).unwrap();
            assert_eq!(key,expected);assert!(verify_registration(&key,&server,&nonce,&decode_hex::<64>(&signature).unwrap()));
            let registered=ServerFrame{version:VERSION,message:Server::Registered{device_id:expected_id.clone()}};
            ws.send(Message::Text(serde_json::to_string(&registered).unwrap().into())).await.unwrap();
            if attempt==1{
                let m=ws.next().await.unwrap().unwrap();
                let c:ClientFrame=serde_json::from_str(m.to_text().unwrap()).unwrap();
                assert!(matches!(c.message,Client::Ping));
                // Drop this transport without granting or resuming a session.
                drop(ws);
            }else{
                let _=tokio::time::timeout(Duration::from_secs(5),ws.next()).await;
            }
        }
    });
    let(stop,rx)=tokio::sync::watch::channel(false);
    let mut online=0;let mut retried=0;
    tokio::time::timeout(Duration::from_secs(12),online_with_retry(Endpoint::development(&url).unwrap(),&id,rx,Duration::from_secs(10),true,|event|{
        match event{
            PresenceLifecycle::Online(actual)=>{assert_eq!(actual,device_id);online+=1;if online==2{let _=stop.send(true);}},
            PresenceLifecycle::Reconnecting(n)=>{assert_eq!(n,1);retried+=1;},
            _=>{},
        }
    })).await.unwrap().unwrap();
    assert_eq!((online,retried),(2,1));
    tokio::time::timeout(Duration::from_secs(6),fixture).await.unwrap().unwrap();
}

#[tokio::test]
async fn shared_controller_identity_is_scoped_to_each_authenticated_socket(){
    use remote_ex_security::session::VIEW;
    let _=rustls::crypto::ring::default_provider().install_default();
    let rcgen::CertifiedKey{cert,key_pair}=rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let config=RustlsConfig::from_pem(cert.pem().into_bytes(),key_pair.serialize_pem().into_bytes()).await.unwrap();
    let mut roots=rustls::RootCertStore::empty();roots.add(cert.der().clone()).unwrap();
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
    let url=format!("wss://localhost:{}/v1/ws",listener.local_addr().unwrap().port());
    let app=remote_ex_rendezvous::router_with_identity([7;32]).unwrap();
    let handle=axum_server::Handle::new();let server=axum_server::from_tcp_rustls(listener,config).unwrap().handle(handle.clone());
    let task=tokio::spawn(async move{server.serve(app.into_make_service()).await.unwrap();});
    tokio::time::timeout(Duration::from_secs(3),handle.listening()).await.unwrap().unwrap();
    let ep=||Endpoint::secure(&url,[7;32]).unwrap();
    let key=DeviceIdentity::generate();
    let mut c1=Connection::register_controller_with_roots(ep(),&key,roots.clone()).await.unwrap();
    let mut c2=Connection::register_controller_with_roots(ep(),&key,roots.clone()).await.unwrap();
    let c3=Connection::register_controller_with_roots(ep(),&key,roots.clone()).await.unwrap();
    assert_eq!(c1.device_id(),c2.device_id());assert_eq!(c1.device_id(),c3.device_id());
    assert!(Connection::register_controller_with_roots(ep(),&key,roots.clone()).await.is_err());
    assert!(Connection::register_with_roots(ep(),&key,roots.clone()).await.is_err());
    let h1_key=DeviceIdentity::generate();let h2_key=DeviceIdentity::generate();
    let mut h1=Connection::register_with_roots(ep(),&h1_key,roots.clone()).await.unwrap();
    let mut h2=Connection::register_with_roots(ep(),&h2_key,roots.clone()).await.unwrap();
    let wait=Duration::from_secs(3);
    // Controller-only sockets cannot be targets.
    h1.send(Client::Request{target:c1.device_id().into(),requested:VIEW,controller_nonce:"01".repeat(32)}).await.unwrap();
    assert!(matches!(h1.receive(wait).await.unwrap(),Server::Error{..}));
    c1.send(Client::Request{target:h1.device_id().into(),requested:VIEW,controller_nonce:"02".repeat(32)}).await.unwrap();
    c2.send(Client::Request{target:h2.device_id().into(),requested:VIEW,controller_nonce:"03".repeat(32)}).await.unwrap();
    let Server::Requested{session:s1,..}=c1.receive(wait).await.unwrap()else{panic!("request1")};
    let Server::Requested{session:s2,..}=c2.receive(wait).await.unwrap()else{panic!("request2")};
    assert_ne!(s1,s2);
    for (host,sid) in [(&mut h1,&s1),(&mut h2,&s2)]{
        assert!(matches!(host.receive(wait).await.unwrap(),Server::Incoming{session,controller,..} if session==*sid&&controller==c1.device_id()));
        host.send(Client::Decision{session:sid.clone(),grant:Some(VIEW),host_nonce:Some("04".repeat(32))}).await.unwrap();
    }
    assert!(matches!(c1.receive(wait).await.unwrap(),Server::Decision{session,..} if session==s1));
    assert!(matches!(c2.receive(wait).await.unwrap(),Server::Decision{session,..} if session==s2));
    c2.send(Client::Close{session:s1.clone()}).await.unwrap();
    assert!(matches!(c2.receive(wait).await.unwrap(),Server::Error{..}));
    c2.send(Client::Signal{session:s1.clone(),payload:remote_ex_rendezvous::protocol::Signal::NoiseHandshake{step:1,packet:"05".repeat(32)}}).await.unwrap();
    assert!(matches!(c2.receive(wait).await.unwrap(),Server::Error{..}));
    c1.close().await.unwrap();
    assert!(matches!(h1.receive(wait).await.unwrap(),Server::Closed{session,..} if session==s1));
    c2.send(Client::Signal{session:s2.clone(),payload:remote_ex_rendezvous::protocol::Signal::NoiseHandshake{step:1,packet:"06".repeat(32)}}).await.unwrap();
    assert!(matches!(h2.receive(wait).await.unwrap(),Server::Signal{session,..} if session==s2));
    c2.send(Client::Close{session:s2.clone()}).await.unwrap();
    assert!(matches!(h2.receive(wait).await.unwrap(),Server::Closed{session,..} if session==s2));
    assert!(matches!(c2.receive(wait).await.unwrap(),Server::Closed{session,..} if session==s2));
    c2.close().await.unwrap();c3.close().await.unwrap();h1.close().await.unwrap();h2.close().await.unwrap();
    handle.graceful_shutdown(Some(Duration::from_secs(1)));
    tokio::time::timeout(Duration::from_secs(3),task).await.unwrap().unwrap();
}

#[tokio::test]
async fn preflight_closes_without_sending_device_registration(){
    use axum::{extract::ws::{Message,WebSocketUpgrade},routing::get,Router};
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    // WSS-only is checked before opening even a loopback plaintext socket.
    let url=format!("ws://{}/v1/ws",listener.local_addr().unwrap());
    assert!(Connection::preflight(Endpoint::development(&url).unwrap()).await.is_err());
    assert!(tokio::time::timeout(Duration::from_millis(100),listener.accept()).await.is_err());
    let _=rustls::crypto::ring::default_provider().install_default();
    let rcgen::CertifiedKey{cert,key_pair}=rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let mut roots=rustls::RootCertStore::empty();roots.add(cert.der().clone()).unwrap();
    let cfg=RustlsConfig::from_pem(cert.pem().into_bytes(),key_pair.serialize_pem().into_bytes()).await.unwrap();
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
    let port=listener.local_addr().unwrap().port();
    let (tx,mut rx)=tokio::sync::mpsc::channel(1);
    let app=Router::new().route("/v1/ws",get(move|ws:WebSocketUpgrade|{
        let tx=tx.clone();
        async move{ws.on_upgrade(move|mut sock|async move{
            let message=serde_json::to_string(&remote_ex_rendezvous::protocol::ServerFrame{version:remote_ex_rendezvous::protocol::VERSION,message:Server::Challenge{server:"07".repeat(32),nonce:"08".repeat(32)}}).unwrap();
            if sock.send(Message::Text(message.into())).await.is_err(){let _=tx.send(false).await;return;}
            let no_registration=matches!(tokio::time::timeout(Duration::from_secs(3),sock.recv()).await,Ok(None)|Ok(Some(Err(_)))|Ok(Some(Ok(Message::Close(_)))));
            let _=tx.send(no_registration).await;
        })}
    }));
    let handle=axum_server::Handle::new();let server=axum_server::from_tcp_rustls(listener,cfg).unwrap().handle(handle.clone());
    let task=tokio::spawn(async move{server.serve(app.into_make_service()).await.unwrap();});
    tokio::time::timeout(Duration::from_secs(3),handle.listening()).await.unwrap().unwrap();
    Connection::preflight_with_roots(Endpoint::secure(&format!("wss://localhost:{port}/v1/ws"),[7;32]).unwrap(),roots).await.unwrap();
    assert_eq!(tokio::time::timeout(Duration::from_secs(4),rx.recv()).await.unwrap(),Some(true));
    handle.graceful_shutdown(Some(Duration::from_secs(1)));tokio::time::timeout(Duration::from_secs(3),task).await.unwrap().unwrap();
}
