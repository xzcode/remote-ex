//! Canonical session proof for a single offer/answer exchange.
//! Verify expected peer keys using a separate trusted pairing mechanism first.
//! Freshness/replay state, human consent, actual DTLS identity verification, and
//! transport authorization remain the responsibility of the session owner.
use super::{DeviceIdentity, Fingerprint, PublicKey};
use ed25519_dalek::{Signature, Signer, VerifyingKey};
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"Remote EX/session-transcript/v1\0";
pub const VIEW: u8 = 1;
pub const INPUT: u8 = 2;
const KNOWN: u8 = VIEW | INPUT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role { Controller = 1, Host = 2 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTranscript {
    pub session_id: [u8; 16],
    pub controller_key: PublicKey,
    pub host_key: PublicKey,
    pub controller_nonce: [u8; 32],
    pub host_nonce: [u8; 32],
    pub requested: u8,
    pub granted: u8,
    // SHA-256 of the exact UTF-8 SDP received, including DTLS fingerprints.
    // Do not normalize or alter SDP after signing.
    pub offer_sha256: Fingerprint,
    pub answer_sha256: Fingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofError {
    InvalidTranscript, WrongIdentity, InvalidSignature, InvalidSdp,
}

pub fn hash_sdp(bytes: &[u8]) -> Result<Fingerprint, ProofError> {
    if bytes.is_empty() || bytes.len() > 64 * 1024 || std::str::from_utf8(bytes).is_err() {
        return Err(ProofError::InvalidSdp);
    }
    Ok(Sha256::digest(bytes).into())
}

impl SessionTranscript {
    pub fn validate(&self) -> Result<(), ProofError> {
        if self.session_id == [0; 16]
            || self.controller_key == [0; 32] || self.host_key == [0; 32]
            || self.controller_key == self.host_key
            || self.controller_nonce == [0; 32] || self.host_nonce == [0; 32]
            || self.controller_nonce == self.host_nonce
            || self.requested & !KNOWN != 0 || self.granted & !KNOWN != 0
            || self.requested & VIEW == 0 || self.granted & VIEW == 0
            || self.granted & !self.requested != 0
            || self.offer_sha256 == [0; 32] || self.answer_sha256 == [0; 32]
        {
            return Err(ProofError::InvalidTranscript);
        }
        Ok(())
    }

    pub fn matches_sdp(&self, offer: &[u8], answer: &[u8]) -> Result<(), ProofError> {
        if hash_sdp(offer)? != self.offer_sha256 || hash_sdp(answer)? != self.answer_sha256 {
            return Err(ProofError::InvalidSdp);
        }
        Ok(())
    }

    fn canonical_bytes(&self, role: Role) -> Result<Vec<u8>, ProofError> {
        self.validate()?;
        // Domain, role, then fixed-width fields in this exact order.
        let mut bytes = Vec::with_capacity(DOMAIN.len() + 1 + 16 + 6 * 32 + 2);
        bytes.extend_from_slice(DOMAIN);
        bytes.push(role as u8);
        bytes.extend_from_slice(&self.session_id);
        bytes.extend_from_slice(&self.controller_key);
        bytes.extend_from_slice(&self.host_key);
        bytes.extend_from_slice(&self.controller_nonce);
        bytes.extend_from_slice(&self.host_nonce);
        bytes.push(self.requested);
        bytes.push(self.granted);
        bytes.extend_from_slice(&self.offer_sha256);
        bytes.extend_from_slice(&self.answer_sha256);
        Ok(bytes)
    }

    fn key(&self, role: Role) -> PublicKey {
        match role { Role::Controller => self.controller_key, Role::Host => self.host_key }
    }
}

impl DeviceIdentity {
    /// Only sign after local user approval and identity checks. This does not
    /// constitute a pairing protocol or approve the actual media transport.
    pub fn sign_session(&self, role: Role, transcript: &SessionTranscript) -> Result<[u8; 64], ProofError> {
        if self.public_key() != transcript.key(role) { return Err(ProofError::WrongIdentity); }
        Ok(self.0.sign(&transcript.canonical_bytes(role)?).to_bytes())
    }
}

/// Both identities proved possession of their expected keys over the same
/// session data. This is NOT evidence that the DTLS transport is connected,
/// that the user approved, or that this transcript has not been replayed.
pub struct VerifiedSession {
    transcript: SessionTranscript,
}

impl VerifiedSession {
    pub fn transcript(&self) -> &SessionTranscript { &self.transcript }
}

fn verify_role(t: &SessionTranscript, role: Role, signature: &[u8; 64]) -> Result<(), ProofError> {
    let key = VerifyingKey::from_bytes(&t.key(role)).map_err(|_| ProofError::WrongIdentity)?;
    key.verify_strict(&t.canonical_bytes(role)?, &Signature::from_bytes(signature))
        .map_err(|_| ProofError::InvalidSignature)
}

/// The expected keys must be obtained independently (verified pairing or
/// pinned trust record), never copied from the untrusted received transcript.
/// Expected nonces/session ID/permissions must also be matched by the caller
/// against local pending state, and consumed exactly once.
pub fn verify_session(
    transcript: SessionTranscript,
    expected_controller: &PublicKey,
    expected_host: &PublicKey,
    offer: &[u8],
    answer: &[u8],
    controller_signature: &[u8; 64],
    host_signature: &[u8; 64],
) -> Result<VerifiedSession, ProofError> {
    transcript.validate()?;
    if &transcript.controller_key != expected_controller || &transcript.host_key != expected_host {
        return Err(ProofError::WrongIdentity);
    }
    transcript.matches_sdp(offer, answer)?;
    verify_role(&transcript, Role::Controller, controller_signature)?;
    verify_role(&transcript, Role::Host, host_signature)?;
    Ok(VerifiedSession { transcript })
}

#[cfg(test)]
mod tests {
    use super::*;
    const OFFER: &[u8] = b"v=0\r\na=fingerprint:sha-256 01:02\r\n";
    const ANSWER: &[u8] = b"v=0\r\na=fingerprint:sha-256 03:04\r\n";
    #[test]
    fn signed_session_binds_roles_keys_grant_and_exact_sdp() {
        let controller = DeviceIdentity::generate();
        let host = DeviceIdentity::generate();
        let t = SessionTranscript {
            session_id: [1;16], controller_key: controller.public_key(), host_key: host.public_key(),
            controller_nonce: [2;32], host_nonce: [3;32], requested: VIEW | INPUT, granted: VIEW,
            offer_sha256: hash_sdp(OFFER).unwrap(), answer_sha256: hash_sdp(ANSWER).unwrap(),
        };
        let a = controller.sign_session(Role::Controller, &t).unwrap();
        let b = host.sign_session(Role::Host, &t).unwrap();
        let check = |t: SessionTranscript, offer: &[u8], a: &[u8;64], b: &[u8;64]| {
            verify_session(t, &controller.public_key(), &host.public_key(), offer, ANSWER, a, b)
        };
        assert_eq!(check(t.clone(), OFFER, &a, &b).unwrap().transcript(), &t);
        assert!(check(t.clone(), b"changed", &a, &b).is_err());
        assert!(check(t.clone(), OFFER, &b, &a).is_err());
        let mut elevated = t.clone();
        elevated.granted = VIEW | INPUT;
        assert!(check(elevated, OFFER, &a, &b).is_err());
        let mut replay = t.clone();
        replay.host_nonce = [4;32];
        assert!(check(replay, OFFER, &a, &b).is_err());
        let mut substituted = t.clone();
        substituted.host_key = DeviceIdentity::generate().public_key();
        assert!(check(substituted, OFFER, &a, &b).is_err());
        assert!(host.sign_session(Role::Controller, &t).is_err());
    }
}
