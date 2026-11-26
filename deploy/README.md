# Remote EX M2 self-host deployment candidate

Do not expose the loopback development server through a proxy. The service can opt in to an enrolled, TLS-only bind. A public deployment requires an explicit nonempty allowlist of device IDs, a CA-trusted server certificate with a matching DNS name, a nonzero independently provisioned server ID and a fixed bind port. Keep the private keys and the TURN shared secret outside the repository. Configure a dedicated unprivileged service account, firewall and resource limits. No shared public/default service is configured by the client.

The executable reads:
- REMOTE_EX_SIGNAL_BIND (default 127.0.0.1:47822)
- REMOTE_EX_PUBLIC_BIND=1 (explicit opt-in only)
- REMOTE_EX_TLS_CERT and REMOTE_EX_TLS_KEY (PEM paths)
- REMOTE_EX_SERVER_ID (64 hex public deployment identifier)
- REMOTE_EX_ALLOWED_DEVICES_FILE (one lowercase full SHA-256 device fingerprint per line; required for public mode, changes require a planned restart)
- REMOTE_EX_STUN_URL (optional stun://host:port)
- REMOTE_EX_TURN_URL and REMOTE_EX_TURN_SECRET_FILE (optional turn[s]://host:port and matching secret file)

All real sessions must still independently authenticate the peer and receive fresh local authorization. Device enrollment is not desktop access. The registration mode refuses all sessions. Separate routed test/view/control clients require a fresh local grant and independently verified keys. Real routed mode is an experimental native candidate, not physical cross-network acceptance. TLS and enrollment restrict access, but Internet DoS resistance and operational security need review. Prefer a dedicated network, restrict inbound WSS/relay ports to authorized sites where possible, and never expose a management subnet to coturn. Do not turn off system security checks.

## TURN
Use the official coturn implementation (https://github.com/coturn/coturn). Install it on a dedicated host/network with a public relay address, or configure the correct external/internal address mapping. Use time-limited REST credentials and the same high-entropy secret as the rendezvous service. The client receives only temporary credentials after both session participants have exchanged encrypted authentication records. Set quota, bandwidth and relay-port limits. Deny private, loopback, multicast, link-local, management, cloud metadata and other nonpublic peer ranges. The example configuration is a template, not an applied deployment.

## Verification
First run the Linux CI's actual local direct and forced TURN synthetic tests. Then test two authorized clients behind different NATs, direct first and forced relay, verify certificates, connectivity and actual relay candidate type; disconnect and re-register with fresh local consent. Only after the full matrix and an operational review should a real remote control service be made reachable. Do not reuse the CI's test credentials, private relay configuration, or generated test certificates.

For enrolled development testing, explicitly prepare a persistent device identity in the desktop Server connection page, independently verify each complete device ID and key, and have the administrator enroll both IDs. The Device ID connection page can use the same explicitly selected identity even for synthetic-only testing. Go offline in the presence page first. Both peers must choose the same profile and relay policy. A saved trusted-device pin is not a session grant. The service can only be deployed after reviewing certificate lifecycle, allowlist, relay network isolation and rate limits. The included systemd example is not enabled automatically.

## Configuration preflight and container candidate

New server builds accept `remote-ex-rendezvous --check-config` with exactly the same environment as startup. It loads the allowlist, validates bind/TLS/ICE requirements, and loads the certificate/key pair without listening on any port. Successful output is a bounded JSON summary (no secret, key, endpoint, or path values). This checks configuration only, not certificate hostname/expiry from a client's viewpoint, DNS, firewall, reachability or relay performance. Startup also rejects invalid opt-in flag values, empty variables, and partially configured TLS. SIGTERM and Ctrl-C terminate the signaling service; WSS has a five-second graceful shutdown deadline.

The container candidate is opt-in, non-root, read-only, capability-dropped, with resource bounds. It contains signaling only; TURN remains an independently configured coturn service. The Compose template publishes only host IPv4 loopback by default, requires explicit private configuration paths, and never starts during installation. No real deployment performed. Build and preflight using an authorized operator's local Docker environment:

```sh
docker compose -f deploy/compose.yml build
# Set REMOTE_EX_CONFIG_ENV and REMOTE_EX_CONFIG_DIR in your shell first.
docker compose -f deploy/compose.yml run --rm --no-deps rendezvous --check-config
```

After review, explicit operator start is `docker compose -f deploy/compose.yml up -d`. For remote access change host bind only after TLS, enrollment and firewall review; never forward plaintext development WS. Do not chmod private keys world-readable. Arrange ownership or restricted ACL for UID 10001. Image uses official Rust and Debian bases, not a pinned/audited production image; no runtime credentials baked in. Verify on disposable container CI before claiming support.
