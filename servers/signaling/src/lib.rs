#![forbid(unsafe_code)]
//! Local development rendezvous. No media relay, remote input, TURN credentials,
//! public deployment or production authentication of the server itself.
mod hub;
pub mod protocol;
pub mod turn;

use axum::{
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use hub::Hub;
use protocol::{Client, ClientFrame, Server, ServerFrame, VERSION};
use rand_core::{OsRng, RngCore};
use remote_ex_security::{DeviceIdentity, fingerprint, verify_registration};
use remote_ex_signaling::{decode_hex, hex};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}},
    time::{Duration, Instant},
};
use tokio::{sync::{Semaphore, mpsc}, time::timeout};

pub use protocol::MAX_MESSAGE;
const MAX_CONNECTIONS: usize = 128;
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT: Duration = Duration::from_secs(3);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct App {
    server_id: [u8;32],
    hub: Arc<Mutex<Hub>>,
    slots: Arc<Semaphore>,
    next_connection: Arc<AtomicU64>,
    allowed_devices: Option<Arc<HashSet<String>>>,
}

pub fn router() -> Router {
    router_with_identity(DeviceIdentity::generate().fingerprint()).expect("generated server identity")
}

/// Stable public server identity configured by the operator. This is an
/// additional application pin, not a substitute for a trusted TLS certificate.
pub fn router_with_identity(server_id: [u8;32]) -> Result<Router, &'static str> {
    router_with_ice(server_id,None)
}

pub fn router_with_ice(server_id:[u8;32],issuer:Option<turn::IceIssuer>)->Result<Router,&'static str>{
    router_configured(server_id,issuer,None)
}

/// Explicit operator enrollment. An absent allowlist is development-only.
pub fn router_configured(server_id:[u8;32],issuer:Option<turn::IceIssuer>,allowed:Option<HashSet<String>>)->Result<Router,&'static str>{
    if allowed.as_ref().is_some_and(|ids|ids.is_empty()||ids.len()>4096||ids.iter().any(|id|!valid_device_id(id))){return Err("invalid device allowlist");}
    if server_id == [0;32] { return Err("invalid server identity"); }
    let hub=match issuer{Some(issuer)=>Hub::with_issuer(issuer),None=>Hub::default()};
    let state = App {
        server_id,
        hub: Arc::new(Mutex::new(hub)),
        slots: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        next_connection: Arc::new(AtomicU64::new(1)),
        allowed_devices:allowed.map(Arc::new),
    };
    Ok(Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/ws", get(upgrade))
        .with_state(state))
}

pub fn valid_device_id(id:&str)->bool{
    id.len()==64&&id.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))&&decode_hex::<32>(id).is_ok_and(|key|key!=[0;32])
}

async fn upgrade(State(app): State<App>, ws: WebSocketUpgrade) -> Response {
    let Ok(permit) = app.slots.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .max_write_buffer_size(2 * MAX_MESSAGE)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            serve(socket, app).await;
        })
}

fn parse(text: &str) -> Result<Client, ()> {
    if text.len() > MAX_MESSAGE { return Err(()); }
    let frame: ClientFrame = serde_json::from_str(text).map_err(|_| ())?;
    if frame.version != VERSION { return Err(()); }
    frame.message.validate().map_err(|_| ())?;
    Ok(frame.message)
}

fn encode(message: Server) -> Result<Message, ()> {
    let data = serde_json::to_string(&ServerFrame { version: VERSION, message }).map_err(|_| ())?;
    if data.len() > MAX_MESSAGE { return Err(()); }
    Ok(Message::Text(data.into()))
}

async fn write<S>(sink: &mut S, message: Server) -> Result<(), ()>
where S: futures_util::Sink<Message> + Unpin {
    let msg = encode(message)?;
    timeout(SEND_TIMEOUT, sink.send(msg)).await.map_err(|_| ())?.map_err(|_| ())
}

async fn serve(socket: WebSocket, app: App) {
    let (mut sink, mut source) = socket.split();
    let mut challenge = [0u8;32];
    OsRng.fill_bytes(&mut challenge);
    if write(&mut sink, Server::Challenge {
        server: hex(&app.server_id), nonce: hex(&challenge),
    }).await.is_err() { return; }

    // Exactly one proof attempt on this socket, before the deadline.
    let proof = timeout(AUTH_TIMEOUT, async {
        while let Some(Ok(message)) = source.next().await {
            match message {
                Message::Text(text) => return parse(text.as_str()),
                Message::Ping(_) | Message::Pong(_) => continue,
                _ => return Err(()),
            }
        }
        Err(())
    }).await;
    let Ok(Ok(Client::Register { public_key, signature })) = proof else { return; };
    let (Ok(key), Ok(sig)) = (decode_hex::<32>(&public_key), decode_hex::<64>(&signature)) else { return; };
    if !verify_registration(&key, &app.server_id, &challenge, &sig) { return; }
    let id = hex(&fingerprint(&key));
    if app.allowed_devices.as_ref().is_some_and(|ids|!ids.contains(&id)){
        let _=write(&mut sink,Server::Error{code:"registration_unavailable".into()}).await;
        return;
    }
    let connection = app.next_connection.fetch_add(1, Ordering::Relaxed);
    let (tx, mut rx) = mpsc::channel(32);
    let registered = app.hub.lock().unwrap_or_else(|e| e.into_inner())
        .register(id.clone(), connection, key, tx);
    if !registered {
        let _ = write(&mut sink, Server::Error { code: "registration_unavailable".into() }).await;
        return;
    }

    // Always unregister the exact connection on socket failure, timeout or close.
    if write(&mut sink, Server::Registered { device_id: id.clone() }).await.is_ok() {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut last = Instant::now();
        let mut rate_start = last;
        let mut count = 0u32;
        loop {
            tokio::select! {
                item = source.next() => {
                    match item {
                        Some(Ok(Message::Text(text))) => {
                            last = Instant::now();
                            if rate_start.elapsed() >= Duration::from_secs(1) {
                                rate_start = Instant::now(); count = 0;
                            }
                            count += 1;
                            if count > 40 { break; }
                            let Ok(message) = parse(text.as_str()) else { break; };
                            if matches!(message, Client::Register { .. }) { break; }
                            let reply = app.hub.lock().unwrap_or_else(|e| e.into_inner())
                                .handle(&id, connection, message, Instant::now());
                            if let Some(reply) = reply {
                                if write(&mut sink, reply).await.is_err() { break; }
                            }
                        }
                        Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => {
                            // WebSocket ping does not extend application presence.
                        }
                        _ => break,
                    }
                }
                item = rx.recv() => {
                    let Some(event) = item else { break; };
                    if write(&mut sink, event).await.is_err() { break; }
                }
                _ = tick.tick() => {
                    app.hub.lock().unwrap_or_else(|e| e.into_inner()).prune(Instant::now());
                    if last.elapsed() >= IDLE_TIMEOUT { break; }
                }
            }
        }
    }
    app.hub.lock().unwrap_or_else(|e| e.into_inner()).disconnect(&id, connection);
    let _ = timeout(SEND_TIMEOUT, sink.send(Message::Close(None))).await;
}
