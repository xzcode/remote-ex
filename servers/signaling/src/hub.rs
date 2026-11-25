//! Bounded routing state. A server grant is only permission to exchange
//! signaling; it NEVER authorizes capture, input, or a trusted media peer.
use crate::protocol::{Client, Server, Signal, grant_valid};
use remote_ex_security::{PublicKey, pairing::new_session_id};
use remote_ex_signaling::hex;
use std::{collections::HashMap, time::{Duration, Instant}};
use tokio::sync::mpsc;

const PENDING_TTL: Duration = Duration::from_secs(45);
const ROUTING_TTL: Duration = Duration::from_secs(120);
const REQUEST_COOLDOWN: Duration = Duration::from_secs(3);
const MAX_ICE_PER_PEER: u16 = 128;

const MAX_SHARED_CONTROLLERS:usize=3;
struct Device {
    id:String,
    controller_only:bool,
    key: PublicKey,
    tx: mpsc::Sender<Server>,
    session: Option<String>,
    last_request: Option<Instant>,
    last_lookup:Option<Instant>,
}
struct Session {
    controller: u64,
    host: u64,
    requested: u8,
    controller_nonce: String,
    grant: Option<u8>,
    deadline: Instant,
    offer: bool,
    answer: bool,
    proofs: [bool;2],
    ice_count: [u16;2],
    noise_step: u8,
    sealed_count: [u8;2],
    ice_issued: [bool;2],
    lease_requested:[bool;2],
    extended:bool,
}
#[derive(Default)]
pub struct Hub {
    devices: HashMap<u64, Device>,
    sessions: HashMap<String, Session>,
    issuer: Option<crate::turn::IceIssuer>,
}
fn error(code: &str) -> Server { Server::Error { code: code.into() } }

impl Hub {
    pub fn with_issuer(issuer:crate::turn::IceIssuer)->Self{Self{issuer:Some(issuer),..Self::default()}}
    #[cfg(test)]
    pub fn register(&mut self,id:String,connection:u64,key:PublicKey,tx:mpsc::Sender<Server>)->bool{
        self.register_scoped(id,connection,key,tx,false)
    }
    pub fn register_scoped(&mut self,id:String,connection:u64,key:PublicKey,tx:mpsc::Sender<Server>,controller_only:bool)->bool{
        if self.devices.len()>=128||self.devices.contains_key(&connection){return false;}
        let peers:Vec<_>=self.devices.values().filter(|d|d.id==id).collect();
        // Host/ordinary registration remains exclusive. Only fresh scoped
        // controller proofs allow up to three independent sockets per ID.
        if !peers.is_empty()&&(!controller_only||peers.iter().any(|d|!d.controller_only||d.key!=key)||peers.len()>=MAX_SHARED_CONTROLLERS){return false;}
        self.devices.insert(connection,Device{id,key,controller_only,tx,session:None,last_request:None,last_lookup:None});true
    }
    fn deliver(&self,connection:u64,event:Server)->bool{
        self.devices.get(&connection).is_some_and(|d|d.tx.try_send(event).is_ok())
    }
    fn finish(&mut self,session:&str,reason:&str){
        let Some(s)=self.sessions.remove(session)else{return;};
        for connection in [s.controller,s.host]{
            if let Some(d)=self.devices.get_mut(&connection){
                if d.session.as_deref()==Some(session){d.session=None;}
                let _=d.tx.try_send(Server::Closed{session:session.into(),reason:reason.into()});
            }
        }
    }
    pub fn prune(&mut self, now: Instant) {
        let expired: Vec<_> = self.sessions.iter().filter(|(_,s)| now >= s.deadline)
            .map(|(id,_)| id.clone()).collect();
        for id in expired { self.finish(&id, "expired"); }
    }
    pub fn disconnect(&mut self, id: &str, connection: u64) {
        if !self.devices.get(&connection).is_some_and(|d|d.id==id){return;}
        if let Some(d)=self.devices.remove(&connection){
            if let Some(session) = d.session { self.finish(&session, "peer_disconnected"); }
        }
    }
    pub fn handle(&mut self, id: &str, connection: u64, message: Client, now: Instant) -> Option<Server> {
        self.prune(now);
        if !self.devices.get(&connection).is_some_and(|d|d.id==id) {
            return Some(error("not_registered"));
        }
        if message.validate().is_err() { return Some(error("invalid_message")); }
        match message {
            Client::Ping => Some(Server::Pong),
            Client::Lookup{targets}=>{
                let d=self.devices.get_mut(&connection).expect("registered");
                if d.last_lookup.is_some_and(|last|now.saturating_duration_since(last)<Duration::from_secs(2)){return Some(error("lookup_limited"));}
                d.last_lookup=Some(now);
                // No directory or public-key disclosure: only exact requested IDs.
                Some(Server::Presence{items:targets.into_iter().map(|device_id|{
                    let online=self.devices.values().any(|d|d.id==device_id);
                    let busy=self.devices.values().any(|d|d.id==device_id&&d.session.is_some());
                    crate::protocol::DeviceStatus{device_id,online,busy}
                }).collect()})
            },
            Client::Register { .. }|Client::RegisterController{..} => Some(error("already_registered")),
            Client::Request { target, requested, controller_nonce } =>
                Some(self.request(connection, target, requested, controller_nonce, now)),
            Client::Decision { session, grant, host_nonce } =>
                self.decision(connection, &session, grant, host_nonce, now),
            Client::Signal { session, payload } => self.signal(connection, &session, payload),
            Client::IceConfig { session } => Some(self.ice_config(connection,&session)),
            Client::ExtendLease{session}=>self.extend_lease(connection,&session,now),
            Client::Close { session } => {
                if self.devices[&connection].session.as_deref() != Some(session.as_str()) {
                    return Some(error("session_unavailable"));
                }
                self.finish(&session, "closed");
                None
            }
        }
    }
    fn request(&mut self, connection:u64, target: String, requested: u8, controller_nonce: String, now: Instant) -> Server {
        let d = self.devices.get_mut(&connection).expect("registered");
        if d.last_request.is_some_and(|last| now.saturating_duration_since(last) < REQUEST_COOLDOWN) {
            return error("request_limited");
        }
        // Charge the request even for nonexistent or unavailable targets.
        d.last_request = Some(now);
        if target == d.id || d.session.is_some() || !grant_valid(requested) {
            return error("request_unavailable");
        }
        let key=d.key;let id=d.id.clone();
        let Some(host)=self.devices.iter().find(|(_,d)|d.id==target&&!d.controller_only&&d.session.is_none()).map(|(&conn,_)|conn)else{return error("request_unavailable");};
        let mut session = hex(&new_session_id());
        while self.sessions.contains_key(&session) { session = hex(&new_session_id()); }
        if !self.deliver(host, Server::Incoming {
            session: session.clone(), controller: id.into(), controller_key: hex(&key), requested,
            controller_nonce: controller_nonce.clone(),
        }) {
            return error("request_unavailable");
        }
        self.devices.get_mut(&connection).unwrap().session=Some(session.clone());
        self.devices.get_mut(&host).unwrap().session=Some(session.clone());
        self.sessions.insert(session.clone(), Session {
            controller:connection,host,requested,controller_nonce,grant:None,
            deadline: now + PENDING_TTL, offer: false, answer: false, proofs: [false;2], ice_count: [0;2], noise_step:0, sealed_count:[0;2], ice_issued:[false;2],lease_requested:[false;2],extended:false,
        });
        Server::Requested { session, target }
    }
    fn decision(&mut self, connection:u64, session: &str, grant: Option<u8>, host_nonce: Option<String>, now: Instant) -> Option<Server> {
        let Some(s) = self.sessions.get_mut(session) else { return Some(error("session_unavailable")); };
        if s.host != connection || s.grant.is_some() { return Some(error("session_unavailable")); }
        if grant.is_some_and(|g| !grant_valid(g) || g & !s.requested != 0)
            || host_nonce.as_ref().is_some_and(|nonce| nonce == &s.controller_nonce) {
            return Some(error("invalid_grant"));
        }
        let controller=s.controller;
        let host_key=hex(&self.devices[&connection].key);
        if let Some(g) = grant {
            s.grant = Some(g);
            s.deadline = now + ROUTING_TTL;
        }
        if !self.deliver(controller, Server::Decision { session: session.into(), grant, host_key, host_nonce }) {
            self.finish(session, "peer_unavailable");
            return Some(error("session_unavailable"));
        }
        if grant.is_none() { self.finish(session, "declined"); }
        None
    }
    fn extend_lease(&mut self,connection:u64,session:&str,now:Instant)->Option<Server>{
        let Some(s)=self.sessions.get_mut(session)else{return Some(error("session_unavailable"));};
        let role=if s.controller==connection{0}else if s.host==connection{1}else{return Some(error("session_unavailable"));};
        if s.grant.is_none()||s.noise_step!=2||s.sealed_count.iter().any(|&n|n==0)||s.ice_issued.iter().any(|&v|v)||s.lease_requested[role]||s.extended{return Some(error("lease_unavailable"));}
        s.lease_requested[role]=true;
        if !s.lease_requested.iter().all(|v|*v){return None;}
        s.extended=true;
        s.deadline=now+Duration::from_secs(u64::from(crate::protocol::EXTENDED_ROUTE_SECONDS));
        let peers=[s.controller,s.host];
        for peer in peers{
            if !self.deliver(peer,Server::LeaseGranted{session:session.into(),seconds:crate::protocol::EXTENDED_ROUTE_SECONDS}){
                self.finish(session,"peer_unavailable");return Some(error("session_unavailable"));
            }
        }
        None
    }
    fn ice_config(&mut self,connection:u64,session:&str)->Server{
        let Some(s)=self.sessions.get_mut(session)else{return error("session_unavailable");};
        let role=if s.controller==connection{0}else if s.host==connection{1}else{return error("session_unavailable");};
        if s.grant.is_none()||s.noise_step!=2||s.sealed_count.iter().any(|&n|n==0)||s.ice_issued[role]||s.lease_requested.iter().any(|&v|v)&&!s.extended{return error("ice_config_unavailable");}
        let config=match &self.issuer{Some(issuer)=>match issuer.issue_for_route(&self.devices[&connection].id,session,s.extended){Ok(c)=>c,Err(_)=>return error("ice_config_unavailable")},None=>crate::protocol::IceConfig::default()};
        s.ice_issued[role]=true;Server::IceConfig{session:session.into(),config}
    }
    fn signal(&mut self, connection:u64, session: &str, payload: Signal) -> Option<Server> {
        let Some(s) = self.sessions.get_mut(session) else { return Some(error("session_unavailable")); };
        let role = if s.controller == connection { 0 } else if s.host == connection { 1 } else {
            return Some(error("session_unavailable"));
        };
        if s.grant.is_none() { return Some(error("session_unavailable")); }
        // First M2 profile retains M1 host-as-offerer convention. Both
        // endpoints must still verify signatures and actual DTLS themselves.
        let valid = match &payload {
            Signal::NoiseHandshake { step:1,.. } if role==0 && s.noise_step==0 && !s.offer => {s.noise_step=1;true}
            Signal::NoiseHandshake { step:2,.. } if role==1 && s.noise_step==1 => {s.noise_step=2;true}
            Signal::Sealed { .. } if s.noise_step==2 && s.sealed_count[role]<24 => {s.sealed_count[role]+=1;true}
            _ if s.noise_step!=0 => false,
            Signal::Offer { .. } if role == 1 && !s.offer => { s.offer = true; true }
            Signal::Answer { .. } if role == 0 && s.offer && !s.answer => { s.answer = true; true }
            Signal::Proof { .. } if s.answer && !s.proofs[role] => { s.proofs[role] = true; true }
            Signal::Ice { .. } if s.proofs.iter().all(|v| *v) && s.ice_count[role] < MAX_ICE_PER_PEER => {
                s.ice_count[role] += 1; true
            }
            Signal::IceBatch { items } if s.proofs.iter().all(|v| *v)
                && s.ice_count[role] == 0 && items.len() <= usize::from(MAX_ICE_PER_PEER) => {
                // Exactly one non-trickle batch; no additional candidates.
                s.ice_count[role] = MAX_ICE_PER_PEER; true
            }
            _ => false,
        };
        if !valid { return Some(error("invalid_signal_order")); }
        let peer=if role==0{s.host}else{s.controller};
        if !self.deliver(peer, Server::Signal { session: session.into(), payload }) {
            self.finish(session, "peer_unavailable");
            return Some(error("session_unavailable"));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use remote_ex_security::{DeviceIdentity, session::{VIEW, INPUT}};
    fn device(h: &mut Hub, connection: u64) -> (String, mpsc::Receiver<Server>) {
        let d = DeviceIdentity::generate();
        let id = hex(&d.fingerprint());
        let (tx, rx) = mpsc::channel(32);
        assert!(h.register(id.clone(), connection, d.public_key(), tx));
        (id, rx)
    }
    #[test]
    fn explicit_host_grant_and_session_isolation() {
        let mut h = Hub::default(); let now = Instant::now();
        let (a, mut arx) = device(&mut h, 1);
        let (b, mut brx) = device(&mut h, 2);
        let (c, _) = device(&mut h, 3);
        let Some(Server::Requested { session, .. }) = h.handle(&a, 1, Client::Request { target:b.clone(), requested:VIEW, controller_nonce:"01".repeat(32) }, now) else { panic!("request"); };
        assert!(matches!(brx.try_recv().unwrap(), Server::Incoming { requested:VIEW, .. }));
        let offer = Client::Signal { session:session.clone(), payload:Signal::Offer { sdp:"fixture".into() } };
        assert!(matches!(h.handle(&b,2,offer.clone(),now),Some(Server::Error{..})));
        assert!(matches!(h.handle(&a,1,Client::Decision{session:session.clone(),grant:Some(VIEW),host_nonce:Some("02".repeat(32))},now),Some(Server::Error{..})));
        assert!(matches!(h.handle(&b,2,Client::Decision{session:session.clone(),grant:Some(VIEW|INPUT),host_nonce:Some("02".repeat(32))},now),Some(Server::Error{..})));
        assert!(matches!(h.handle(&b,2,Client::Decision{session:session.clone(),grant:Some(VIEW),host_nonce:Some("01".repeat(32))},now),Some(Server::Error{..})));
        assert!(h.handle(&b,2,Client::Decision{session:session.clone(),grant:Some(VIEW),host_nonce:Some("02".repeat(32))},now).is_none());
        assert!(matches!(arx.try_recv().unwrap(),Server::Decision{grant:Some(VIEW),..}));
        assert!(matches!(h.handle(&c,3,offer.clone(),now),Some(Server::Error{..})));
        assert!(h.handle(&b,2,offer.clone(),now).is_none());
        assert!(matches!(arx.try_recv().unwrap(),Server::Signal{..}));
        assert!(matches!(h.handle(&b,2,offer,now),Some(Server::Error{..})));
        h.disconnect(&b,2);
        assert!(matches!(arx.try_recv().unwrap(),Server::Closed{..}));
        assert!(h.sessions.is_empty());
    }
    #[test]
    fn presence_lookup_requires_exact_ids_and_is_rate_limited(){
        let mut h=Hub::default();let now=Instant::now();
        let(a,_)=device(&mut h,1);let(b,_)=device(&mut h,2);
        let targets=vec![b.clone(),"aa".repeat(32)];
        let Some(Server::Presence{items})=h.handle(&a,1,Client::Lookup{targets:targets.clone()},now)else{panic!("lookup")};
        assert!(items[0].online);assert!(!items[1].online);
        assert!(matches!(h.handle(&a,1,Client::Lookup{targets},now),Some(Server::Error{..})));
    }
    #[test]
    fn extended_lease_requires_both_exact_route_members_and_never_renews(){
        let mut h=Hub::default();let now=Instant::now();let(a,mut arx)=device(&mut h,1);let(b,mut brx)=device(&mut h,2);let(c,_)=device(&mut h,3);
        let Some(Server::Requested{session,..})=h.handle(&a,1,Client::Request{target:b.clone(),requested:VIEW,controller_nonce:"01".repeat(32)},now)else{panic!("request")};
        brx.try_recv().unwrap();
        let lease=Client::ExtendLease{session:session.clone()};
        assert!(matches!(h.handle(&a,1,lease.clone(),now),Some(Server::Error{..})));
        h.handle(&b,2,Client::Decision{session:session.clone(),grant:Some(VIEW),host_nonce:Some("02".repeat(32))},now);
        arx.try_recv().unwrap();
        {let s=h.sessions.get_mut(&session).unwrap();s.noise_step=2;s.sealed_count=[1,1];}
        assert!(h.handle(&a,1,lease.clone(),now).is_none());
        assert!(arx.try_recv().is_err());assert!(!h.sessions[&session].extended);
        assert!(matches!(h.handle(&c,3,lease.clone(),now),Some(Server::Error{..})));
        assert!(matches!(h.handle(&a,1,lease.clone(),now),Some(Server::Error{..})));
        assert!(matches!(h.handle(&a,1,Client::IceConfig{session:session.clone()},now),Some(Server::Error{..})));
        assert!(h.handle(&b,2,lease.clone(),now).is_none());
        assert!(matches!(arx.try_recv().unwrap(),Server::LeaseGranted{seconds:420,..}));
        assert!(matches!(brx.try_recv().unwrap(),Server::LeaseGranted{seconds:420,..}));
        assert!(matches!(h.handle(&b,2,lease,now),Some(Server::Error{..})));
        h.prune(now+Duration::from_secs(121));assert!(h.sessions.contains_key(&session));
        h.prune(now+Duration::from_secs(420));assert!(h.sessions.is_empty());
    }
    #[test]
    fn pending_expiry_and_rate_limit() {
        let mut h = Hub::default(); let now = Instant::now();
        let (a, mut arx) = device(&mut h,1); let (b, mut brx) = device(&mut h,2);
        let request = Client::Request { target:b, requested:VIEW, controller_nonce:"01".repeat(32) };
        assert!(matches!(h.handle(&a,1,request.clone(),now),Some(Server::Requested{..})));
        assert!(matches!(h.handle(&a,1,request.clone(),now),Some(Server::Error{..})));
        brx.try_recv().unwrap();
        h.prune(now + PENDING_TTL);
        assert!(matches!(arx.try_recv().unwrap(),Server::Closed{..}));
        assert!(matches!(brx.try_recv().unwrap(),Server::Closed{..}));
        assert!(matches!(h.handle(&a,1,request,now+PENDING_TTL),Some(Server::Requested{..})));
    }
}
