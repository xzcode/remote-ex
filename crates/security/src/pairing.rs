//! Single-use local pairing state. No network listener or automatic approval.
//! The verified peer public key must be supplied by an independently trusted
//! full-fingerprint comparison or a pinned device trust record.
use crate::{DeviceIdentity, PublicKey, Role, SessionTranscript, VerifiedSession, hash_sdp, verify_session};
use crate::session::{INPUT, VIEW, ProofError};
use rand_core::{OsRng, RngCore};
use std::time::{Duration, Instant};

const MAX_AGE: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingError {
    InvalidRequest, Expired, WrongIdentity, Proof(ProofError),
}
impl From<ProofError> for PairingError {
    fn from(error: ProofError) -> Self { Self::Proof(error) }
}
pub fn new_session_id() -> [u8; 16] {
    let mut id = [0; 16];
    while id == [0; 16] { OsRng.fill_bytes(&mut id); }
    id
}
fn fresh_nonce() -> [u8; 32] {
    let mut nonce = [0; 32];
    while nonce == [0; 32] { OsRng.fill_bytes(&mut nonce); }
    nonce
}
fn valid_permissions(flags: u8) -> bool {
    flags & VIEW != 0 && flags & !(VIEW | INPUT) == 0
}

/// Outbound requests require a fresh challenge before the rendezvous server
/// assigns its routing ID. Only this locally generated nonce and pinned peer
/// can be bound to the returned session ID, exactly once.
pub struct OutgoingPairing {
    local_key: PublicKey,
    peer_key: PublicKey,
    nonce: [u8;32],
    requested: u8,
    created: Instant,
}
impl OutgoingPairing {
    pub fn new(identity: &DeviceIdentity, independently_verified_peer: PublicKey, requested: u8) -> Result<Self, PairingError> {
        let local_key = identity.public_key();
        if independently_verified_peer == [0;32] || independently_verified_peer == local_key || !valid_permissions(requested) {
            return Err(PairingError::InvalidRequest);
        }
        Ok(Self { local_key, peer_key: independently_verified_peer, nonce: fresh_nonce(), requested, created: Instant::now() })
    }
    pub fn nonce(&self) -> [u8;32] { self.nonce }
    pub fn bind_session(self, session_id: [u8;16]) -> Result<PendingPairing, PairingError> {
        if session_id == [0;16] { return Err(PairingError::InvalidRequest); }
        if self.created.elapsed() >= MAX_AGE { return Err(PairingError::Expired); }
        Ok(PendingPairing { role: Role::Controller, session_id, local_key: self.local_key, peer_key: self.peer_key,
            local_nonce: self.nonce, requested: self.requested, created: self.created })
    }
}

/// Pending state is deliberately not Clone; signing consumes this instance.
pub struct PendingPairing {
    role: Role,
    session_id: [u8; 16],
    local_key: PublicKey,
    peer_key: PublicKey,
    local_nonce: [u8; 32],
    requested: u8,
    created: Instant,
}

impl PendingPairing {
    pub fn new(
        role: Role, session_id: [u8; 16], identity: &DeviceIdentity,
        independently_verified_peer: PublicKey, requested: u8,
    ) -> Result<Self, PairingError> {
        let local_key = identity.public_key();
        if session_id == [0; 16] || independently_verified_peer == [0; 32]
            || independently_verified_peer == local_key || !valid_permissions(requested)
        { return Err(PairingError::InvalidRequest); }
        Ok(Self {
            role, session_id, local_key, peer_key: independently_verified_peer,
            local_nonce: fresh_nonce(), requested, created: Instant::now(),
        })
    }
    pub fn nonce(&self) -> [u8; 32] { self.local_nonce }

    /// Called exclusively by the local consent UI/owner, never a network request.
    /// Both sides must confirm the exact grant and verified peer identity.
    pub fn approve_local(self, granted: u8) -> Result<ApprovedPairing, PairingError> {
        self.check_age()?;
        if !valid_permissions(granted) || granted & !self.requested != 0 {
            return Err(PairingError::InvalidRequest);
        }
        Ok(ApprovedPairing { pending: self, granted })
    }

    fn check_age(&self) -> Result<(), PairingError> {
        if self.created.elapsed() >= MAX_AGE { Err(PairingError::Expired) } else { Ok(()) }
    }
}

pub struct ApprovedPairing {
    pending: PendingPairing,
    granted: u8,
}
impl ApprovedPairing {
    /// Sign exactly one session transcript; a retry requires a new pairing.
    /// The supplied SDP must be the exact strings to be used by the media engine.
    pub fn sign(
        self, identity: &DeviceIdentity, peer_nonce: [u8; 32],
        offer: &[u8], answer: &[u8],
    ) -> Result<SignedPairing, PairingError> {
        self.pending.check_age()?;
        if identity.public_key() != self.pending.local_key { return Err(PairingError::WrongIdentity); }
        let p = self.pending;
        let (controller_key, host_key, controller_nonce, host_nonce) = match p.role {
            Role::Controller => (p.local_key, p.peer_key, p.local_nonce, peer_nonce),
            Role::Host => (p.peer_key, p.local_key, peer_nonce, p.local_nonce),
        };
        let transcript = SessionTranscript {
            session_id: p.session_id, controller_key, host_key, controller_nonce, host_nonce,
            requested: p.requested, granted: self.granted,
            offer_sha256: hash_sdp(offer)?, answer_sha256: hash_sdp(answer)?,
        };
        let signature = identity.sign_session(p.role, &transcript)?;
        Ok(SignedPairing { pending: p, transcript, signature })
    }
}

pub struct SignedPairing {
    pending: PendingPairing,
    transcript: SessionTranscript,
    signature: [u8; 64],
}
impl SignedPairing {
    pub fn signature(&self) -> [u8; 64] { self.signature }

    /// Consumes the signed attempt even on failure. No replay or second
    /// completion on this instance. The session owner still checks that
    /// the negotiated DTLS transport matches the verified SDP.
    pub fn finish(
        self, peer_signature: &[u8; 64], offer: &[u8], answer: &[u8],
    ) -> Result<VerifiedSession, PairingError> {
        self.pending.check_age()?;
        let (controller_key, host_key, controller_sig, host_sig) = match self.pending.role {
            Role::Controller => (self.pending.local_key, self.pending.peer_key, &self.signature, peer_signature),
            Role::Host => (self.pending.peer_key, self.pending.local_key, peer_signature, &self.signature),
        };
        Ok(verify_session(self.transcript, &controller_key, &host_key, offer, answer,
            controller_sig, host_sig)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const OFFER: &[u8] = b"offer";
    const ANSWER: &[u8] = b"answer";
    #[test]
    fn both_sides_confirm_same_grant_and_fresh_nonces() {
        let controller = DeviceIdentity::generate();
        let host = DeviceIdentity::generate();
        let id = new_session_id();
        let c = PendingPairing::new(Role::Controller, id, &controller, host.public_key(), VIEW | INPUT).unwrap();
        let h = PendingPairing::new(Role::Host, id, &host, controller.public_key(), VIEW | INPUT).unwrap();
        let cn = c.nonce();
        let hn = h.nonce();
        let c = c.approve_local(VIEW).unwrap().sign(&controller, hn, OFFER, ANSWER).unwrap();
        let h = h.approve_local(VIEW).unwrap().sign(&host, cn, OFFER, ANSWER).unwrap();
        let cs = c.signature();
        let hs = h.signature();
        assert_eq!(c.finish(&hs, OFFER, ANSWER).unwrap().transcript().granted, VIEW);
        assert_eq!(h.finish(&cs, OFFER, ANSWER).unwrap().transcript().session_id, id);
        // A new attempt with the same session ID still has a fresh nonce,
        // so replaying the old host signature cannot authorize it.
        let retry = PendingPairing::new(Role::Controller, id, &controller, host.public_key(), VIEW | INPUT).unwrap();
        assert_ne!(retry.nonce(), cn);
        let retry = retry.approve_local(VIEW).unwrap().sign(&controller, hn, OFFER, ANSWER).unwrap();
        assert!(retry.finish(&hs, OFFER, ANSWER).is_err());
    }
    #[test]
    fn routed_request_keeps_local_nonce_and_original_deadline() {
        let a = DeviceIdentity::generate();
        let b = DeviceIdentity::generate();
        let request = OutgoingPairing::new(&a,b.public_key(),VIEW).unwrap();
        let nonce = request.nonce();
        let pending = request.bind_session(new_session_id()).unwrap();
        assert_eq!(pending.nonce(),nonce);
        let mut request = OutgoingPairing::new(&a,b.public_key(),VIEW).unwrap();
        request.created = Instant::now() - MAX_AGE;
        assert!(matches!(request.bind_session(new_session_id()),Err(PairingError::Expired)));
        assert!(OutgoingPairing::new(&a,b.public_key(),VIEW).unwrap().bind_session([0;16]).is_err());
    }
    #[test]
    fn escalation_and_expiration_are_rejected() {
        let a = DeviceIdentity::generate();
        let b = DeviceIdentity::generate();
        let p = PendingPairing::new(Role::Host, new_session_id(), &a, b.public_key(), VIEW).unwrap();
        assert!(matches!(p.approve_local(VIEW | INPUT), Err(PairingError::InvalidRequest)));
        let mut p = PendingPairing::new(Role::Host, new_session_id(), &a, b.public_key(), VIEW).unwrap();
        p.created = Instant::now() - MAX_AGE;
        assert!(matches!(p.approve_local(VIEW), Err(PairingError::Expired)));
    }
}
