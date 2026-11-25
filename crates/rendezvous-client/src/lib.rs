#![forbid(unsafe_code)]
//! Authenticated registration client. WSS requires verified CA/hostname
//! and a separately configured application server ID. The only plaintext
//! option is explicitly selected numeric IPv4 loopback development.
pub mod secure_route;
use futures_util::{SinkExt,StreamExt};
use remote_ex_rendezvous_protocol::{Client,ClientFrame,Server,ServerFrame,VERSION,MAX_MESSAGE};
use remote_ex_security::{DeviceIdentity,Fingerprint};
use remote_ex_signaling::{decode_hex,hex};
use std::{collections::VecDeque,sync::Arc,time::Duration};
use tokio::{net::TcpStream,time::timeout};
use tokio_tungstenite::{connect_async_tls_with_config,Connector,MaybeTlsStream,WebSocketStream,tungstenite::{Message,protocol::WebSocketConfig}};
use url::{Url,Host};

pub type Result<T> = std::result::Result<T,String>;
const CONNECT_LIMIT: Duration = Duration::from_secs(10);
const IO_LIMIT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Endpoint { url: Url, expected: Option<Fingerprint> }
impl Endpoint {
    fn parse(value:&str)->Result<Url>{
        let u=Url::parse(value).map_err(|_|"Invalid signaling URL")?;
        if !u.username().is_empty()||u.password().is_some()||u.query().is_some()||u.fragment().is_some()||u.path()!="/v1/ws"||u.host().is_none(){
            return Err("Use a server URL with /v1/ws, no credentials, query or fragment".into());
        }
        Ok(u)
    }
    pub fn secure(value:&str,expected:Fingerprint)->Result<Self>{
        let url=Self::parse(value)?;
        if url.scheme()!="wss"||expected==[0;32]{return Err("WSS and a configured nonzero server identity are required".into());}
        Ok(Self{url,expected:Some(expected)})
    }
    pub fn development(value:&str)->Result<Self>{
        let url=Self::parse(value)?;
        if url.scheme()!="ws"||!matches!(url.host(),Some(Host::Ipv4(ip)) if ip.is_loopback()){
            return Err("Plain WebSocket is restricted to explicit numeric IPv4 loopback development".into());
        }
        Ok(Self{url,expected:None})
    }
}
pub struct Connection {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    device_id: String,
    failed: bool,
    pending:VecDeque<Server>,
}
impl Connection {
    pub async fn register(endpoint: Endpoint, identity:&DeviceIdentity)->Result<Self>{
        let roots=rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        Self::register_with_roots(endpoint,identity,roots).await
    }
    /// Explicit CA trust configuration (e.g. a private organization's CA).
    /// Hostname, validity, chain and signature verification remain enabled.
    pub async fn register_with_roots(endpoint:Endpoint,identity:&DeviceIdentity,roots:rustls::RootCertStore)->Result<Self>{
        let tls=rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions().map_err(|_|"Invalid TLS configuration")?
            .with_root_certificates(roots).with_no_client_auth();
        let cfg=WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE)).max_frame_size(Some(MAX_MESSAGE)).max_write_buffer_size(2*MAX_MESSAGE);
        let (ws,_)=timeout(CONNECT_LIMIT,connect_async_tls_with_config(endpoint.url.as_str(),Some(cfg),true,Some(Connector::Rustls(Arc::new(tls)))))
            .await.map_err(|_|"Signaling connection timed out")?.map_err(|_|"Signaling connection or TLS verification failed")?;
        let mut conn=Self{ws,device_id:hex(&identity.fingerprint()),failed:false,pending:VecDeque::new()};
        let Server::Challenge{server,nonce}=conn.receive(IO_LIMIT).await? else{return Err("Expected registration challenge".into());};
        let server=decode_hex::<32>(&server).map_err(|_|"Invalid server identity")?;
        let nonce=decode_hex::<32>(&nonce).map_err(|_|"Invalid registration nonce")?;
        if endpoint.expected.is_some_and(|expected|expected!=server){return Err("Configured server identity mismatch".into());}
        let proof=identity.sign_registration(&server,&nonce);
        conn.send_inner(Client::Register{public_key:hex(&identity.public_key()),signature:hex(&proof)}).await?;
        match conn.receive(IO_LIMIT).await?{
            Server::Registered{device_id} if device_id==conn.device_id=>Ok(conn),
            _=>Err("Server rejected registration or changed the device identity".into()),
        }
    }
    pub fn device_id(&self)->&str{&self.device_id}
    async fn send_inner(&mut self,message:Client)->Result<()>{
        if self.failed{return Err("Signaling closed".into());}
        message.validate().map_err(str::to_owned)?;
        let data=serde_json::to_string(&ClientFrame{version:VERSION,message}).map_err(|_|"Cannot encode signaling message")?;
        if data.len()>MAX_MESSAGE{return Err("Signaling message exceeds limit".into());}
        let result=timeout(IO_LIMIT,self.ws.send(Message::Text(data.into()))).await;
        if !matches!(result,Ok(Ok(()))){self.failed=true;return Err("Signaling write failed".into());}
        Ok(())
    }
    pub async fn send(&mut self,message:Client)->Result<()>{
        if matches!(message,Client::Register{..}){return Err("Already registered".into());}
        self.send_inner(message).await
    }
    pub async fn receive(&mut self,limit:Duration)->Result<Server>{
        if self.failed{return Err("Signaling closed".into());}
        if let Some(event)=self.pending.pop_front(){return Ok(event);}
        self.receive_network(limit).await
    }
    async fn receive_network(&mut self,limit:Duration)->Result<Server>{
        if self.failed{return Err("Signaling closed".into());}
        let limit=limit.min(Duration::from_secs(60));
        let result=timeout(limit,async{
            loop{
                match self.ws.next().await{
                    Some(Ok(Message::Text(data)))=>{
                        if data.len()>MAX_MESSAGE{return Err("Signaling message exceeds limit".to_string());}
                        let frame:ServerFrame=serde_json::from_str(data.as_str()).map_err(|_|"Invalid signaling response")?;
                        if frame.version!=VERSION{return Err("Signaling version mismatch".into());}
                        frame.message.validate().map_err(str::to_owned)?;
                        return Ok(frame.message);
                    }
                    Some(Ok(Message::Ping(_)))=>{self.ws.flush().await.map_err(|_|"WebSocket pong failed")?;}
                    Some(Ok(Message::Pong(_)))=>{},
                    Some(Ok(Message::Close(_)))|None|Some(Err(_))=>return Err("Signaling transport closed".into()),
                    _=>return Err("Invalid signaling frame".into()),
                }
            }
        }).await;
        match result {
            Ok(Ok(message))=>Ok(message),
            Ok(Err(e))=>{self.failed=true;Err(e)},
            Err(_)=>{self.failed=true;Err("Signaling receive timed out".into())}
        }
    }
    /// Request operator-configured ICE credentials once after the encrypted
    /// handshake. Preserve up to four racing ciphertext records in order.
    pub async fn request_ice(&mut self,session:&str)->Result<remote_ex_rendezvous_protocol::IceConfig>{
        if !self.pending.is_empty(){return Err("Unexpected pending records before ICE configuration".into());}
        self.send(Client::IceConfig{session:session.into()}).await?;
        for _ in 0..5{
            match self.receive_network(IO_LIMIT).await?{
                Server::IceConfig{session:id,config} if id==session=>return Ok(config),
                event @ Server::Signal{session:_,payload:remote_ex_rendezvous_protocol::Signal::Sealed{..}}=>{
                    if !matches!(&event,Server::Signal{session:id,..} if id==session){self.failed=true;return Err("Wrong ICE session".into());}
                    self.pending.push_back(event);
                },
                _=>{self.failed=true;return Err("Unexpected ICE configuration response".into());}
            }
        }
        self.failed=true;Err("ICE configuration record limit exceeded".into())
    }
    pub async fn close(mut self)->Result<()>{
        timeout(IO_LIMIT,self.ws.close(None)).await.map_err(|_|"Signaling close timed out")?.map_err(|_|"Signaling close failed".into())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn endpoints_fail_closed(){
        assert!(Endpoint::development("ws://127.0.0.1:47822/v1/ws").is_ok());
        assert!(Endpoint::development("ws://192.168.1.2:47822/v1/ws").is_err());
        assert!(Endpoint::development("ws://localhost:47822/v1/ws").is_err());
        assert!(Endpoint::secure("ws://127.0.0.1/v1/ws",[1;32]).is_err());
        assert!(Endpoint::secure("wss://example.com/v1/ws",[0;32]).is_err());
        assert!(Endpoint::secure("wss://user@example.com/v1/ws",[1;32]).is_err());
        assert!(Endpoint::secure("wss://example.com/v1/ws",[1;32]).is_ok());
    }
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub enum PresenceEvent { Heartbeat, RequestDeclined, Devices(Vec<remote_ex_rendezvous_protocol::DeviceStatus>) }

/// Explicit presence only. No acceptance of routed sessions, no capture,
/// no input or auto-reconnect. A single outstanding Ping must receive a Pong
/// within five seconds; unrelated frames cannot extend this deadline.
pub async fn presence(
    conn:&mut Connection,stop:tokio::sync::watch::Receiver<bool>,duration:Duration,event:impl FnMut(PresenceEvent),
)->Result<()>{presence_queries(conn,stop,duration,None,event).await}
async fn presence_queries(
    conn:&mut Connection,mut stop:tokio::sync::watch::Receiver<bool>,duration:Duration,
    mut queries:Option<&mut tokio::sync::mpsc::Receiver<Vec<String>>>,mut event:impl FnMut(PresenceEvent),
)->Result<()>{
    use tokio::time::{Instant, interval, MissedTickBehavior};
    let deadline=Instant::now()+duration.min(Duration::from_secs(300));
    let mut tick=interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut next_ping=Instant::now();
    let mut awaiting:Option<Instant>=None;
    let mut lookup:Option<(Instant,Vec<String>)>=None;
    let mut next_lookup=Instant::now();
    loop{
        if *stop.borrow()||Instant::now()>=deadline{return Ok(());}
        tokio::select!{
            biased;
            changed=stop.changed()=>{
                if changed.is_err()||*stop.borrow(){return Ok(());}
            },
            _=tick.tick()=>{
                let now=Instant::now();
                if now>=deadline{return Ok(());}
                if awaiting.is_some_and(|due|now>=due){return Err("Presence heartbeat timed out".into());}
                if lookup.as_ref().is_some_and(|(due,_)|now>=*due){return Err("Presence lookup timed out".into());}
                if awaiting.is_none()&&now>=next_ping{
                    conn.send(Client::Ping).await?;
                    awaiting=Some(Instant::now()+Duration::from_secs(5));
                    next_ping=Instant::now()+Duration::from_secs(10);
                }
            },
            query=async{match queries.as_deref_mut(){Some(q)=>q.recv().await,None=>std::future::pending().await}},
                if lookup.is_none()&&Instant::now()>=next_lookup=>{
                if let Some(targets)=query{
                    conn.send(Client::Lookup{targets:targets.clone()}).await?;
                    lookup=Some((Instant::now()+Duration::from_secs(5),targets));
                    next_lookup=Instant::now()+Duration::from_secs(2);
                }else{queries=None;}
            },
            response=conn.receive(Duration::from_secs(30))=>{
                match response?{
                    Server::Pong if awaiting.take().is_some()=>event(PresenceEvent::Heartbeat),
                    Server::Presence{items}=>{
                        let Some((_,targets))=lookup.take()else{return Err("Unexpected presence lookup result".into());};
                        if !items.iter().map(|d|d.device_id.as_str()).eq(targets.iter().map(String::as_str)){return Err("Mismatched presence lookup".into());}
                        event(PresenceEvent::Devices(items));
                    },
                    Server::Incoming{session,..}=>{
                        conn.send(Client::Decision{session,grant:None,host_nonce:None}).await?;
                        event(PresenceEvent::RequestDeclined);
                    }
                    Server::Closed{..}=>{},
                    _=>return Err("Unexpected presence response".into()),
                }
            }
        }
    }
}

#[derive(Clone,Debug,PartialEq,Eq)]
pub enum PresenceLifecycle { Online(String), Heartbeat, RequestDeclined, Reconnecting(u8), Devices(Vec<remote_ex_rendezvous_protocol::DeviceStatus>) }

/// Retry presence only after an established connection fails. Authentication,
/// TLS, protocol or pin mismatches are terminal. Re-registration uses the same
/// locally owned identity but never resumes a session or prior input grant.
pub async fn online_with_retry(
    endpoint:Endpoint,identity:&DeviceIdentity,stop:tokio::sync::watch::Receiver<bool>,duration:Duration,retry:bool,event:impl FnMut(PresenceLifecycle),
)->Result<()>{online_with_queries(endpoint,identity,stop,duration,retry,None,event).await}
pub async fn online_with_queries(
    endpoint:Endpoint,identity:&DeviceIdentity,mut stop:tokio::sync::watch::Receiver<bool>,
    duration:Duration,retry:bool,mut queries:Option<&mut tokio::sync::mpsc::Receiver<Vec<String>>>,mut event:impl FnMut(PresenceLifecycle),
)->Result<()>{
    use tokio::time::Instant;
    let deadline=Instant::now()+duration.min(Duration::from_secs(300));
    let mut attempts=0u8;
    loop{
        if *stop.borrow()||Instant::now()>=deadline{return Ok(());}
        let mut conn=tokio::select!{
            biased;
            _=stop.changed()=>return Ok(()),
            _=tokio::time::sleep_until(deadline)=>return Ok(()),
            result=Connection::register(endpoint.clone(),identity)=>result?,
        };
        event(PresenceLifecycle::Online(conn.device_id().to_owned()));
        let remaining=deadline.saturating_duration_since(Instant::now());
        let result=presence_queries(&mut conn,stop.clone(),remaining,queries.as_deref_mut(),|e|event(match e{
            PresenceEvent::Heartbeat=>PresenceLifecycle::Heartbeat,
            PresenceEvent::RequestDeclined=>PresenceLifecycle::RequestDeclined,
            PresenceEvent::Devices(items)=>PresenceLifecycle::Devices(items),
        })).await;
        let close=conn.close().await;
        match result{
            Ok(())=>return if *stop.borrow(){Ok(())}else{close},
            Err(e)=>{
                if !retry||attempts>=3||!matches!(e.as_str(),"Signaling transport closed"|"Signaling write failed"|"Presence heartbeat timed out"|"Signaling receive timed out"){return Err(e);}
                attempts+=1;
                event(PresenceLifecycle::Reconnecting(attempts));
                tokio::select!{
                    biased;
                    _=stop.changed()=>return Ok(()),
                    _=tokio::time::sleep_until(deadline)=>return Ok(()),
                    _=tokio::time::sleep(Duration::from_secs(1u64<<attempts))=>{},
                }
            }
        }
    }
}
