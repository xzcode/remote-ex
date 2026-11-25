//! Conservative SHA-256 SDP certificate pinning for the single-video session.
//! Require all advertised fingerprints to agree, including any data m-line.
use sha2::{Digest, Sha256};
use crate::MediaResult;

fn fingerprint(sdp: &str) -> MediaResult<[u8;32]> {
    if sdp.is_empty() || sdp.len() > 64 * 1024 { return Err("Invalid signed SDP length".into()); }
    let mut found = None;
    let mut sections = 0;
    let mut session_has = false;
    let mut section_has = false;
    for line in sdp.lines() {
        if line.starts_with("m=") {
            if sections > 0 && !session_has && !section_has {
                return Err("Missing media fingerprint".into());
            }
            sections += 1;
            section_has = false;
        }
        if let Some(raw) = line.strip_prefix("a=fingerprint:") {
            let mut fields = raw.split_ascii_whitespace();
            if !fields.next().is_some_and(|x| x.eq_ignore_ascii_case("sha-256")) {
                return Err("Only SHA-256 DTLS fingerprints are supported".into());
            }
            let raw = fields.next().ok_or("Missing fingerprint value")?;
            if fields.next().is_some() || raw.len() != 95 { return Err("Invalid DTLS fingerprint".into()); }
            let mut bytes = [0;32];
            let mut parts = raw.split(':');
            for byte in &mut bytes {
                let pair = parts.next().ok_or("Short fingerprint")?;
                if pair.len() != 2 { return Err("Invalid fingerprint component".into()); }
                *byte = u8::from_str_radix(pair, 16).map_err(|_| "Invalid fingerprint hex")?;
            }
            if parts.next().is_some() || found.is_some_and(|old| old != bytes) {
                return Err("Conflicting fingerprints".into());
            }
            found = Some(bytes);
            if sections == 0 { session_has = true; } else { section_has = true; }
        }
    }
    if sections == 0 || (!session_has && !section_has) { return Err("Missing media fingerprint".into()); }
    found.ok_or_else(|| "Missing SHA-256 fingerprint".into())
}
pub fn verify(sdp: &str, certificate_pem: &str) -> MediaResult<()> {
    if certificate_pem.len() > 64 * 1024 { return Err("DTLS certificate exceeds limit".into()); }
    let expected = fingerprint(sdp)?;
    let certs = pem::parse_many(certificate_pem).map_err(|_| "Invalid DTLS certificate PEM")?;
    // Single self-signed leaf expected for this profile.
    if certs.len() != 1 || certs[0].tag() != "CERTIFICATE" || certs[0].contents().is_empty() {
        return Err("Missing or ambiguous DTLS certificate".into());
    }
    let actual: [u8;32] = Sha256::digest(certs[0].contents()).into();
    if actual != expected { return Err("DTLS certificate fingerprint does not match signed SDP".into()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_mismatched_or_conflicting_fingerprints() {
        let cert = pem::encode(&pem::Pem::new("CERTIFICATE", b"fixture".to_vec()));
        let hash = Sha256::digest(b"fixture").iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":");
        let valid = format!("v=0\r\na=fingerprint:sha-256 {hash}\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n");
        assert!(verify(&valid, &cert).is_ok());
        assert!(verify("v=0\r\nm=video 9 UDP/TLS/RTP/SAVPF 96\r\n", &cert).is_err());
        assert!(verify(&valid, &pem::encode(&pem::Pem::new("CERTIFICATE", b"other".to_vec()))).is_err());
        assert!(verify(&format!("{valid}m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\na=fingerprint:sha-256 {}\r\n", "00:".repeat(31) + "00"), &cert).is_err());
    }
}
