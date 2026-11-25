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

struct Device {
    connection: u64,
    key: PublicKey,
    tx: mpsc::Sender<Server>,
    session: Option<String>,
    last_request: Option<Instant>,
}
struct Session {
    controller: String,
    host: String,
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
}
#[derive(Default)]
pub struct Hub {
    devices: HashMap<String, Device>,
    sessions: HashMap<String, Session>,
}
fn error(code: &str) -> Server { Server::Error { code: code.into() } }

impl Hub {
    pub fn register(&mut self, id: String, connection: u64, key: PublicKey, tx: mpsc::Sender<Server>) -> bool {
        if self.devices.len() >= 128 || self.devices.contains_key(&id) { return false; }
        self.devices.insert(id, Device { connection, key, tx, session: None, last_request: None });
        true
    }
    fn deliver(&self, id: &str, event: Server) -> bool {
        self.devices.get(id).is_some_and(|d| d.tx.try_send(event).is_ok())
    }
    fn finish(&mut self, session: &str, reason: &str) {
        let Some(s) = self.sessions.remove(session) else { return; };
        for id in [s.controller, s.host] {
            if let Some(d) = self.devices.get_mut(&id) {
                if d.session.as_deref() == Some(session) { d.session = None; }
                // An overloaded destination is not allowed to queue unbounded
                // notifications. Endpoint deadlines must also fail closed.
                let _ = d.tx.try_send(Server::Closed { session: session.into(), reason: reason.into() });
            }
        }
    }
    pub fn prune(&mut self, now: Instant) {
        let expired: Vec<_> = self.sessions.iter().filter(|(_,s)| now >= s.deadline)
            .map(|(id,_)| id.clone()).collect();
        for id in expired { self.finish(&id, "expired"); }
    }
    pub fn disconnect(&mut self, id: &str, connection: u64) {
        if !self.devices.get(id).is_some_and(|d| d.connection == connection) { return; }
        if let Some(d) = self.devices.remove(id) {
            if let Some(session) = d.session { self.finish(&session, "peer_disconnected"); }
        }
    }
    pub fn handle(&mut self, id: &str, connection: u64, message: Client, now: Instant) -> Option<Server> {
        self.prune(now);
        if !self.devices.get(id).is_some_and(|d| d.connection == connection) {
            return Some(error("not_registered"));
        }
        if message.validate().is_err() { return Some(error("invalid_message")); }
        match message {
            Client::Ping => Some(Server::Pong),
            Client::Register { .. } => Some(error("already_registered")),
            Client::Request { target, requested, controller_nonce } =>
                Some(self.request(id, target, requested, controller_nonce, now)),
            Client::Decision { session, grant, host_nonce } =>
                self.decision(id, &session, grant, host_nonce, now),
            Client::Signal { session, payload } => self.signal(id, &session, payload),
            Client::Close { session } => {
                if self.devices[id].session.as_deref() != Some(session.as_str()) {
                    return Some(error("session_unavailable"));
                }
                self.finish(&session, "closed");
                None
            }
        }
    }
    fn request(&mut self, id: &str, target: String, requested: u8, controller_nonce: String, now: Instant) -> Server {
        let d = self.devices.get_mut(id).expect("registered");
        if d.last_request.is_some_and(|last| now.saturating_duration_since(last) < REQUEST_COOLDOWN) {
            return error("request_limited");
        }
        // Charge the request even for nonexistent or unavailable targets.
        d.last_request = Some(now);
        if target == id || d.session.is_some() || !grant_valid(requested) {
            return error("request_unavailable");
        }
        let key = d.key;
        if !self.devices.get(&target).is_some_and(|d| d.session.is_none()) {
            return error("request_unavailable");
        }
        let mut session = hex(&new_session_id());
        while self.sessions.contains_key(&session) { session = hex(&new_session_id()); }
        if !self.deliver(&target, Server::Incoming {
            session: session.clone(), controller: id.into(), controller_key: hex(&key), requested,
            controller_nonce: controller_nonce.clone(),
        }) {
            return error("request_unavailable");
        }
        self.devices.get_mut(id).unwrap().session = Some(session.clone());
        self.devices.get_mut(&target).unwrap().session = Some(session.clone());
        self.sessions.insert(session.clone(), Session {
            controller: id.into(), host: target.clone(), requested, controller_nonce, grant: None,
            deadline: now + PENDING_TTL, offer: false, answer: false, proofs: [false;2], ice_count: [0;2], noise_step:0, sealed_count:[0;2],
        });
        Server::Requested { session, target }
    }
    fn decision(&mut self, id: &str, session: &str, grant: Option<u8>, host_nonce: Option<String>, now: Instant) -> Option<Server> {
        let Some(s) = self.sessions.get_mut(session) else { return Some(error("session_unavailable")); };
        if s.host != id || s.grant.is_some() { return Some(error("session_unavailable")); }
        if grant.is_some_and(|g| !grant_valid(g) || g & !s.requested != 0)
            || host_nonce.as_ref().is_some_and(|nonce| nonce == &s.controller_nonce) {
            return Some(error("invalid_grant"));
        }
        let controller = s.controller.clone();
        let host_key = hex(&self.devices[id].key);
        if let Some(g) = grant {
            s.grant = Some(g);
            s.deadline = now + ROUTING_TTL;
        }
        if !self.deliver(&controller, Server::Decision { session: session.into(), grant, host_key, host_nonce }) {
            self.finish(session, "peer_unavailable");
            return Some(error("session_unavailable"));
        }
        if grant.is_none() { self.finish(session, "declined"); }
        None
    }
    fn signal(&mut self, id: &str, session: &str, payload: Signal) -> Option<Server> {
        let Some(s) = self.sessions.get_mut(session) else { return Some(error("session_unavailable")); };
        let role = if s.controller == id { 0 } else if s.host == id { 1 } else {
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
        let peer = if role == 0 { s.host.clone() } else { s.controller.clone() };
        if !self.deliver(&peer, Server::Signal { session: session.into(), payload }) {
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
