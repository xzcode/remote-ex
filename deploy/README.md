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


## M6 local operator commands (new source candidate)

Use a matching new server binary; alpha.2 does not include these commands. They are offline and never modify active configuration or restart the server.

```sh
remote-ex-rendezvous --protocol-info
remote-ex-rendezvous --allowlist-info /path/to/current-allowlist.txt
remote-ex-rendezvous --new-allowlist /path/to/new-list.txt DEVICE_ID PUBLIC_KEY
remote-ex-rendezvous --enroll-plan /path/to/current-allowlist.txt /path/to/staged-list.txt DEVICE_ID PUBLIC_KEY
remote-ex-rendezvous --revoke-plan /path/to/current-allowlist.txt /path/to/staged-revocation.txt DEVICE_ID
remote-ex-rendezvous --allowlist-diff /path/to/current-allowlist.txt /path/to/staged-list.txt
```

Replace placeholders with the exact independently verified full lowercase hex IDs and public keys of devices you administer. Enrollment only permits registration, not desktop access. The operator commands verify public-key format and fingerprint match, bound and normalize the list, create a new staging file without overwriting anything, and return counts/hash rather than IDs or credentials. Refuse empty lists. Review the staged file independently, confirm the active canonical hash has not changed, use a private directory with restricted permissions, then deliberately select the new file in the service environment, preflight, and schedule a restart. Revocation becomes effective only after the reviewed config is applied and service restarted; no immediate live-session revocation claim.

## Optional multi-TURN pool (new source candidate)

Use either the existing REMOTE_EX_TURN_URL + REMOTE_EX_TURN_SECRET_FILE pair, or REMOTE_EX_RELAY_POOL_FILE (never both). See relay-pool.json.example; provide real reviewed relay DNS names and private absolute secret file paths outside the repository. Optional REMOTE_EX_RELAY_REGION filters exact lowercase region. A maximum of eight relay entries, unique IDs and URLs, priority 1–1000 (lower is preferred), all secret files validated at startup. Within the best priority and selected region, a stable hash of the server-generated session ID distributes routes across eligible relays. Both peers get the same relay and separate short-lived scoped credentials, through unchanged v1 ICE config. No automatic region guessing, network health measurement or failover; unavailable selected region fails configuration. --check-config reports total/eligible counts only. Reconfigure only after a planned restart.

The new pool validates DNS-name shape and rejects IP literals and obvious local name suffixes. This is not DNS resolution or proof of public addressing. The operator must verify actual public A/AAAA answers and relay certificate validity, protect DNS, firewall and coturn peer/egress restrictions, and never route to management/metadata/private networks. No active probing or public service deployment by this code. The legacy single-relay path still supports isolated local test fixtures. Actual multi-relay cross-network acceptance remains pending.


## Opt-in browser Origin policy (new M6 source candidate)

WebSocket requests carrying an Origin header are refused by default. To allow a specific browser origin, set REMOTE_EX_WEB_ORIGIN to its canonical exact HTTPS origin (no path, trailing slash, wildcard, credentials, query or fragment). The server requires TLS and an explicit nonempty device allowlist in this mode. For an explicitly loopback-only WSS fixture (no public opt-in), numeric IPv4 loopback HTTP origins are permitted, e.g. http://127.0.0.1:8080. No-Origin native requests retain their existing challenge/allowlist authentication; Origin is not a device identity or an authorization credential, and all connections still require a valid proof of possession. Do not enable an arbitrary unreviewed Web origin or forward plaintext WS. The new browser integration uses a disposable local TLS service and ephemeral test CA only; no public Web controller is supplied or deployed. This setting is not included in alpha.2.

### Multiple reviewed browser origins (M6 source candidate)

Instead of the legacy single REMOTE_EX_WEB_ORIGIN, configure REMOTE_EX_WEB_ORIGINS as a comma-separated list of one to eight exact canonical origins (no spaces, duplicates, wildcard, path or trailing slash). For example, include https://appassets.androidplatform.net for the packaged Android client and the separately reviewed HTTPS site used by the iOS controller. Never set both environment variables. TLS and a nonempty device allowlist remain mandatory, and enrollment remains separate from each host's local session approval. Preflight reports web_origin_count without listing private configuration. The new list is applied only on a reviewed restart; this is not live enrollment or a public administration API. The default still rejects browser Origin headers. A disposable Chromium fixture checks two permitted origins and an unlisted origin.

### Offline controller-site inspection

After staging a new mobile/browser HTTPS asset directory, run `node scripts/verify-mobile-stage.mjs /path/to/staged-directory`. This checks the exact regular-file allowlist, canonical operator policy, generated security headers, WASM magic/version and landing page, then emits a SHA-256 tree digest, file count, total bytes and mode without printing endpoint or credential data. It does not deploy, validate code signatures, prove the actual reverse proxy's headers or perform network checks. Keep the inspected tree immutable and serve the reviewed headers verbatim over trusted HTTPS. Re-run after changes, review deployment config and run the actual client/relay checks before enabling live access. Extra files, symlinks or weakened headers fail.

### Dedicated static HTTPS controller service (M6 source candidate)

Use a verified staged controller directory, not the localhost diagnostic lab. The new `deploy/controller-site.mjs` is a separate Node 22+ static HTTPS service. It reads exactly the staged file allowlist into a bounded immutable snapshot, recomputes the verified tree digest and canonical response headers, validates the local TLS key/certificate match, hostname and validity, and serves only fixed GET/HEAD paths (including correct WASM MIME). No uploads, file browser, signaling proxy, credential endpoints or fixture code. Unknown paths/hosts/methods are rejected. The default bind is 127.0.0.1:8443; a non-loopback bind requires explicit public opt-in, reviewed enabled operator config and a separately provisioned DNS certificate. No public service is deployed or automatically started. Node TLS trust at the client is still required; local preflight does not prove the full certificate chain or network.

1. Build WASM and stage reviewed operator configuration in a fresh directory (see apps/mobile/README.md). Inspect the tree with the existing verifier.
2. Review controller-site.env.example, keep TLS material outside the served directory, set the reviewed environment variables in the operator service account environment.
3. Run `node deploy/controller-site.mjs --check-config /path/to/staged-directory`; this checks configuration without listening. Review exact HTTPS origin (including port except 443), configure rendezvous REMOTE_EX_WEB_ORIGIN(S), and enroll device IDs separately.
4. Explicitly start `node deploy/controller-site.mjs --serve /path/to/staged-directory` only after review. Use a dedicated unprivileged account and restricted firewall. The systemd example defaults to no capabilities and an unprivileged port; use a reviewed TLS reverse proxy or separately approved low-port privilege arrangement if needed. A reverse proxy must preserve the reviewed Host and not weaken headers; the backend stays HTTPS, not the diagnostic lab. No automatic restart, deployment or certificate issuance. To update assets, stage a new immutable directory, preflight, change the configured directory and perform a planned restart; keep the last verified directory for rollback.
5. Perform actual browser/mobile TLS, enrollment and media acceptance. This new service's isolated TLS test uses disposable synthetic assets, not a physical connection test.
