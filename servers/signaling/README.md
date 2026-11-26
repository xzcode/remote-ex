# M2.1 rendezvous service (development only)

`cargo run -p remote-ex-rendezvous` starts an Axum server on 127.0.0.1:47822, with `GET /healthz` and `/v1/ws`. REMOTE_EX_SIGNAL_BIND accepts only IPv4 loopback. No deployment, public bind, TLS or real client integration yet. Do not expose this prototype via a reverse proxy. The server identity is ephemeral, not an independently trusted certificate.

The client receives a fresh per-socket challenge and uses its existing Ed25519 DeviceIdentity to sign the server fingerprint and nonce. The device ID is the full lowercase SHA-256 fingerprint of the verified public key (64 hex characters). No public device directory, password or anonymous identity claim. Duplicate online ID refused. Future client storage must persist per-install keys securely; currently there is no database or production persistent identity.

The server handles one pending/active routing session per device. Only the authenticated host may grant a subset of the requested capabilities. Controller and host exchange independently generated nonzero nonces, and the server rejects identical nonces. Endpoints must retain local freshness checks and sign their exact transcript. Pending 45 seconds; approved routing 120 seconds; both can close. Disconnect and expiry remove state. Versioned bounded typed JSON, per-connection challenge timeout, queue, connection and message limits, rate and request cooldown. Only session participants can exchange host offer, controller answer, one proof per peer, bounded ICE; no renegotiation or application commands. This server does not verify both SDP proofs itself, issue media authority, relay video/input, or provide trusted peer identity. Endpoints must independently verify pinned public keys, user consent, transcript signatures and actual DTLS before sensitive actions. These routing messages are not the final protocol.

`cargo test -p remote-ex-rendezvous` runs state checks and a real local WebSocket registration/request/decline/invalid-proof test. Production prerequisites: trusted TLS/WSS client verification, persistent identities and trust records, authenticated registration enrollment and rate limits per source, encrypted/signed end-to-end negotiation, robust reconnect and revocation, STUN/TURN configuration/short-lived relay credentials and cross-network test. No M2 network feature is claimed ready until these are integrated.

## TLS development mode and registration client

The same server can now use TLS on IPv4 loopback. Set REMOTE_EX_TLS_CERT and REMOTE_EX_TLS_KEY to the operator-provided PEM certificate chain and key and REMOTE_EX_SERVER_ID to a separately provisioned nonzero 32-byte identity encoded as 64 hex characters. All three are required; partial TLS configuration fails without downgrade. Keep private keys outside the repository. The client must independently configure this server ID and validate its certificate/hostname. For a private CA, explicitly select its certificate file rather than disabling validation. No Internet bind is enabled.

```
cargo run -p remote-ex-rendezvous-client --bin register_device -- --dev ws://127.0.0.1:47822/v1/ws
cargo run -p remote-ex-rendezvous-client --bin register_device -- --wss wss://localhost:47822/v1/ws SERVER_ID --ca CA_PEM
```

The CLI registers, heartbeats and declines all incoming requests for up to 30 seconds. It does not initiate a remote session. On Windows/macOS only, build with `--features persistent` and add `--persistent` to the WSS command to explicitly opt into OS-protected persistent device identity. The local consent prompt is mandatory; there is no startup credential access, automatic enrollment, seed export or fallback to an ephemeral identity after storage failure. Linux persistent store and desktop integration are pending. Default CLI identity remains temporary. Persistent ID is a routing identifier, not remote control authorization. The TLS integration test uses a generated test certificate and checks trusted CA/hostname, wrong server ID, duplicate registration and heartbeat.


M2.3 adds an optional non-trickle IceBatch message, allowed once per participant after both proofs; the existing single ICE message remains supported for development. The new synthetic media integration exercises the actual TLS service, independent pinned endpoint keys, registered device-ID routing and WebRTC certificate verification. See crates/media/README.md. Neither the service nor presence GUI enables real remote control.


M3 shared outgoing controller registration uses a separately signed RegisterController challenge. Up to three controller-only sockets for the same fingerprint are allowed; they cannot receive incoming requests, and are mutually exclusive with ordinary Register for that ID. Every session is owned by the exact socket, and one socket disconnect closes only its own session. Clients must use verified WSS, explicitly choose shared identity, and freshly approve each session. Existing single-connection registration remains supported. Requires updating the server and client together for shared mode; no silent downgrade. Actual Internet/physical parallel session acceptance remains pending.


M6 first candidate adds offline protocol/allowlist management and bounded operator TURN pool. See deploy/README.md and docs/M6_FIRST_HANDOFF.md. No live management endpoint or deployed service. Alpha.2 is published; these new features need new builds.

M6 multi-client configuration adds REMOTE_EX_WEB_ORIGINS (up to eight exact entries, mutually exclusive with REMOTE_EX_WEB_ORIGIN). TLS, device allowlist, signed registration and local host approval remain required. No configuration is enabled by default; see deploy/README.md.
