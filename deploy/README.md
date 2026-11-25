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

All real sessions must still independently authenticate the peer and receive fresh local authorization. Device enrollment is not desktop access. The registration and routed diagnostic currently allow only test media; do not assume that this service has passed physical cross-network acceptance. TLS and enrollment restrict access, but Internet DoS resistance and operational security need review. Prefer a dedicated network, restrict inbound WSS/relay ports to authorized sites where possible, and never expose a management subnet to coturn. Do not turn off system security checks.

## TURN
Use the official coturn implementation (https://github.com/coturn/coturn). Install it on a dedicated host/network with a public relay address, or configure the correct external/internal address mapping. Use time-limited REST credentials and the same high-entropy secret as the rendezvous service. The client receives only temporary credentials after both session participants have exchanged encrypted authentication records. Set quota, bandwidth and relay-port limits. Deny private, loopback, multicast, link-local, management, cloud metadata and other nonpublic peer ranges. The example configuration is a template, not an applied deployment.

## Verification
First run the Linux CI's actual local direct and forced TURN synthetic tests. Then test two authorized clients behind different NATs, direct first and forced relay, verify certificates, connectivity and actual relay candidate type; disconnect and re-register with fresh local consent. Only after the full matrix and an operational review should a real remote control service be made reachable. Do not reuse the CI's test credentials, private relay configuration, or generated test certificates.
