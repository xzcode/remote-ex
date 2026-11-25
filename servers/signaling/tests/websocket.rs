use futures_util::{SinkExt, StreamExt};
use remote_ex_rendezvous::protocol::{Client, ClientFrame, Server, ServerFrame, VERSION};
use remote_ex_security::{DeviceIdentity, session::VIEW};
use remote_ex_signaling::{decode_hex, hex};
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;
use std::time::Duration;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
async fn send(ws: &mut Socket, message: Client) {
    ws.send(Message::Text(serde_json::to_string(&ClientFrame { version: VERSION, message }).unwrap().into())).await.unwrap();
}
async fn receive(ws: &mut Socket) -> Server {
    let message = tokio::time::timeout(Duration::from_secs(3), ws.next()).await.unwrap().unwrap().unwrap();
    let frame: ServerFrame = serde_json::from_str(message.to_text().unwrap()).unwrap();
    assert_eq!(frame.version, VERSION); frame.message
}
async fn registered(url: &str) -> (Socket, String) {
    let (mut ws, _) = connect_async(url).await.unwrap();
    let Server::Challenge { server, nonce } = receive(&mut ws).await else { panic!("challenge"); };
    let identity = DeviceIdentity::generate();
    let signature = identity.sign_registration(&decode_hex::<32>(&server).unwrap(), &decode_hex::<32>(&nonce).unwrap());
    send(&mut ws, Client::Register { public_key:hex(&identity.public_key()), signature:hex(&signature) }).await;
    let Server::Registered { device_id } = receive(&mut ws).await else { panic!("registered"); };
    assert_eq!(device_id, hex(&identity.fingerprint()));
    (ws,device_id)
}
#[tokio::test]
async fn real_websocket_registration_request_decline_and_invalid_proof() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/v1/ws", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, remote_ex_rendezvous::router()).await.unwrap(); });
    let (mut a,_) = registered(&url).await;
    let (mut b, bid) = registered(&url).await;
    send(&mut a, Client::Request { target:bid, requested:VIEW }).await;
    let Server::Requested { session, .. } = receive(&mut a).await else { panic!("request"); };
    assert!(matches!(receive(&mut b).await, Server::Incoming { .. }));
    send(&mut b, Client::Decision { session, grant:None }).await;
    assert!(matches!(receive(&mut a).await, Server::Decision { grant:None, .. }));
    assert!(matches!(receive(&mut a).await, Server::Closed { .. }));
    assert!(matches!(receive(&mut b).await, Server::Closed { .. }));

    let (mut bad,_) = connect_async(&url).await.unwrap();
    assert!(matches!(receive(&mut bad).await, Server::Challenge { .. }));
    let key = DeviceIdentity::generate();
    send(&mut bad, Client::Register { public_key:hex(&key.public_key()), signature:hex(&[0;64]) }).await;
    let result = tokio::time::timeout(Duration::from_secs(3),bad.next()).await.unwrap();
    assert!(!matches!(result, Some(Ok(Message::Text(_)))));
    task.abort();
}
