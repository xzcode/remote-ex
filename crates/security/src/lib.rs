#![forbid(unsafe_code)]
//! Device identity and session transcript primitives. No production pairing or network handshake.
pub mod session;
pub use session::{SessionTranscript, Role, VerifiedSession, ProofError, hash_sdp, verify_session};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

pub type PublicKey = [u8; 32];
pub type Fingerprint = [u8; 32];
const REGISTER_DOMAIN: &[u8] = b"Remote EX/register-proof/v1\0";

pub struct DeviceIdentity(SigningKey);
impl DeviceIdentity {
    pub fn generate() -> Self { Self(SigningKey::generate(&mut OsRng)) }
    pub fn public_key(&self) -> PublicKey { self.0.verifying_key().to_bytes() }
    pub fn fingerprint(&self) -> Fingerprint { fingerprint(&self.public_key()) }
    /// Proof of possession primitive. The caller must authenticate the server,
    /// issue fresh one-use challenges and bind registration to the connection.
    pub fn sign_registration(&self, server_identity: &Fingerprint, challenge: &[u8; 32]) -> [u8; 64] {
        self.0.sign(&registration_bytes(server_identity, challenge)).to_bytes()
    }
}
pub fn fingerprint(public: &PublicKey) -> Fingerprint {
    Sha256::digest(public).into()
}
pub fn verify_registration(public: &PublicKey, server_identity: &Fingerprint, challenge: &[u8; 32], signature: &[u8; 64]) -> bool {
    let Ok(key) = VerifyingKey::from_bytes(public) else { return false; };
    key.verify_strict(&registration_bytes(server_identity, challenge), &Signature::from_bytes(signature)).is_ok()
}
fn registration_bytes(server_identity: &Fingerprint, challenge: &[u8; 32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(REGISTER_DOMAIN.len() + 64);
    bytes.extend_from_slice(REGISTER_DOMAIN);
    bytes.extend_from_slice(server_identity);
    bytes.extend_from_slice(challenge);
    bytes
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proof_binds_key_server_and_challenge() {
        let id = DeviceIdentity::generate();
        let other = DeviceIdentity::generate();
        let server = [3;32];
        let challenge = [4;32];
        let proof = id.sign_registration(&server, &challenge);
        assert!(verify_registration(&id.public_key(), &server, &challenge, &proof));
        assert!(!verify_registration(&other.public_key(), &server, &challenge, &proof));
        assert!(!verify_registration(&id.public_key(), &[5;32], &challenge, &proof));
        assert!(!verify_registration(&id.public_key(), &server, &[5;32], &proof));
        assert_ne!(id.fingerprint(), other.fingerprint());
    }
}
