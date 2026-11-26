# Remote EX v1 cross-client contract baseline

Scope: existing desktop v1, not a supported browser/mobile control implementation. Read crates/rendezvous-protocol, crates/security, crates/rendezvous-client, crates/protocol and routed_session for executable behavior.

## Export and fixtures
- `remote-ex-rendezvous --protocol-info` prints current versions, wire bounds, Noise pattern, profile matrix and explicit unsupported Web/mobile state, without a network connection.
- v1-fixture.json contains only synthetic public keys, public signatures, artificial transcript inputs and an inert Protobuf ReleaseAll record. No private keys or real endpoint data. SDP strings are deliberately not usable WebRTC descriptions; noise_hash is a synthetic fixed value, not an actual Noise handshake.
- Rust contract_vectors tests exercise production verification/decoder functions; Node WebCrypto script independently verifies the public signatures and exact fixed-width encoding. This does not prove a browser can establish Noise transport, bind actual SDP/DTLS, or implement consent/freshness gates.

## Existing protocol requirements
JSON ClientFrame/ServerFrame version 1; WSS with verified TLS and independently configured server ID. Registration proves a fresh challenge using Ed25519 and domain-separated bytes (ordinary vs controller-only). Device ID = SHA-256 of complete raw Ed25519 public key. Unknown fields/messages and oversized input fail validation. Relay only routes signaling; it grants no desktop permission.

The route binding is the UTF-8 literal "Remote EX/routed-noise/v1" followed by NUL, session[16], controller and host public keys[32] each, controller and host nonce[32] each, requested/granted/profile/relay-only bytes in that order. The verified session signs its exact bound context, role (1 controller / 2 host) and Noise handshake hash[32]. Noise NN uses 25519/ChaChaPoly/BLAKE2s, signed by independently verified peer keys; the first sealed record is a signature. No SDP, ICE or media authority until mutual proof verification. A separate domain-separated SDP transcript signs exact UTF-8 SDP hashes and both nonces/permissions/keys. Actual DTLS peer fingerprints must still match. No plaintext downgrade.

Profile metadata is shared in crates/security/src/profiles.rs. Codes 0–27 and existing verifier behavior are retained. Synthetic profile 0 has historical VIEW or VIEW|INPUT verification compatibility; the actual synthetic session owner uses VIEW|INPUT. This is not real input authority. Every other profile has an exact signed mask. Profile-specific, local consent and platform gates remain separate. InputEnvelope v1 is Protobuf with 16-byte session ID, nonzero sequence, bounded event payload; only a verified and approved active session may dispatch. See actual decoder for field and value bounds. Unknown profile must be rejected. No new browser/mobile-specific profiles or wire changes in this batch.

## Browser implementation gate
The fixed public-vector WebCrypto check is only the first portion of M6.4. Before any real WebRTC media or input, establish independent browser-equivalent Noise state, authenticated route and SDP verification with actual DTLS binding, ephemeral/persistent identity storage, explicit consent/revocation, resource bounds and compatibility fixtures. Fail closed if a primitive or browser API is unavailable.
