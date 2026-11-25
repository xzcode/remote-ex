# Native media engine (M1 development)

Independent Rust crate using GStreamer 1.22+ / gstreamer-rs 0.23. VP8/RTP, WebRTC DTLS-SRTP/ICE, sender and receiver, bounded ICE event queues, SDP offer/answer primitives, native RGBA appsrc input, native decoded sample output. The library primitives have no production identity or permission owner. The separate LAN diagnostic integrates test-only pairing, a single explicit private/loopback TCP endpoint, and a reliable data channel. No production signaling, STUN/TURN configuration, screen streaming or OS input. Do not expose this prototype as a remote control service.

The loopback executable uses two real WebRTC endpoints in a single process and an internally generated moving pattern. It checks successful peer states and at least 30 decoded frames. It does not measure real screen-to-screen latency. The media pipeline consumes native GStreamer buffers; no frame-by-frame Tauri IPC.

## Ubuntu
```sh
sudo apt-get install -y pkg-config libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-nice
cargo test --manifest-path crates/media/Cargo.toml
cargo run --manifest-path crates/media/Cargo.toml --bin loopback
```

Windows/macOS: install the matching native GStreamer development/runtime packages and configure pkg-config/library discovery according to upstream installation docs. Builds, actual hardware capture, renderer and packaging are still pending.

References: https://gstreamer.freedesktop.org/documentation/webrtc/ and https://gstreamer.freedesktop.org/documentation/app/appsrc.html. The application code is original; GStreamer and gstreamer-rs dependency licenses, native plugins and codec distribution obligations still require review.

## Integration contract
- The authorized session owner must verify peer identity and bind the DTLS fingerprint, local user consent, grant and session before exposing a real frame source.
- Validate/limit incoming SDP and ICE at the authenticated signaling boundary; queue candidates until the remote description is applied. The prototype is one video m-line, no renegotiation.
- Use a dedicated media worker, not the UI thread. Stop on revoke, disconnect, overflow, failure or timeout. Keep the native raw input bounded.
- Native capture adapter supplies a tightly packed RGBA buffer at configured resolution; scaling and actual platform capture are separate. Native renderer should consume GstSample with correct stride and GPU integration.

## M1 native input and rendering diagnostics
- Empty ICE candidates now represent end-of-candidates, consistent with the GStreamer 1.24+ API: https://gstreamer.freedesktop.org/releases/1.24/ and https://gstreamer.freedesktop.org/documentation/webrtc/.
- `cargo run --manifest-path crates/media/Cargo.toml --bin loopback -- --native-pattern` feeds moving native RGBA data through appsrc, RTP/ICE/DTLS, decoding, and appsink. This is synthetic data.
- `cargo run --manifest-path crates/media/Cargo.toml --bin loopback -- --preview` opens a native video sink on a graphical desktop, not a webview. It runs at most 30 seconds after negotiation. Closing the terminal stops the diagnostic.
- Optional `local-screen` feature uses XCap for an interactive local-only real screen diagnostic. Requires the platform's normal screen capture permission, XCap development dependencies, GStreamer native libraries, and a display ID from Remote EX diagnostics. Run `cargo run --manifest-path crates/media/Cargo.toml --features local-screen --bin loopback -- --screen DISPLAY_ID` and type CAPTURE at the interactive confirmation. Real capture starts only after the two internally linked peers report Connected. No external signaling or screen recording, no remote input. Native sink and actual capture are pending physical-machine verification. This intentionally slow one-frame-at-a-time prototype is not the production capture engine.

## Independent-process LAN diagnostic (synthetic video only)

Requires GStreamer 1.22+, including SCTP/data-channel support. Build the CLI:
```sh
cargo build --manifest-path crates/media/Cargo.toml --bin lan_diagnostic
python3 scripts/test_lan_diagnostic.py crates/media/target/debug/lan_diagnostic
```
The Python harness starts two processes on IPv4 loopback and checks both exit successfully. On Windows use the .exe path. The host sends a synthetic video pattern; the controller receives decoded frames and transmits exactly three fixed Protobuf input messages; the host validates them with SessionGate and returns a diagnostic acknowledgement. No real input is injected.

For manual testing on two owned machines on a trusted private LAN, use the host's actual RFC1918 IP address and an available port, not a wildcard or public address. Run the same CLI with `--listen IP:PORT` on the host and `--connect IP:PORT` on the controller. Both processes print their ephemeral public keys; exchange/verify the full public keys independently and enter the corresponding peer key, then enter ALLOW DIAGNOSTIC on each side. Approve the listener first. The host binds only after local confirmation and accepts exactly one connection; both processes exit at the end or on error. This diagnostic is not a secure production listener (TCP signaling is unencrypted). It does not send real desktop content, permit arbitrary remote input, retain keys, or launch services. Never port-forward it to the Internet.

The input data channel is reliable ordered SCTP with a fixed negotiated ID, bounded binary payload and queue. See upstream https://gstreamer.freedesktop.org/documentation/webrtc/ and https://gstreamer.freedesktop.org/documentation/webrtclib/gstwebrtc-datachannel.html. Actual LAN and platform testing still required.

Controller-only optional `--preview` opens the native GStreamer synthetic video window for about five seconds after connection (subject to errors). The CLI emits CONNECTED only after signed SDP verification and the WebRTC peer/channel are ready. The desktop supervisor consumes these fixed milestones, not arbitrary commands. Native window rendering is not verified by headless CI.

## Encrypted view-only LAN prototype (M1)

Separate `lan_view` executable uses Noise KK encrypted signaling (independently pinned ephemeral X25519 keys), existing Ed25519 session signatures, exact signed SDP, and a SHA-256 comparison against the certificate exposed by the actual video DTLS transport. Both sides exchange encrypted readiness before the host sends any real desktop pixels. No data channel, OS input, clipboard, files, or unattended mode. The old unencrypted lan_diagnostic remains synthetic-only.

Test using two processes on loopback:
```sh
cargo build --manifest-path crates/media/Cargo.toml --bin lan_view
python3 scripts/test_secure_view.py crates/media/target/debug/lan_view
```
On two owned, authorized desktop machines with GStreamer and platform capture prerequisites, build with `--features local-screen`. The host runs `lan_view --listen PRIVATE_IP:PORT --screen DISPLAY_ID`; the controller runs `lan_view --connect PRIVATE_IP:PORT --view`. The host explicitly chooses a display ID from local diagnostics. Both exchange and independently verify the whole 128-character public key bundle and type the exact consent phrase displayed in each visible interactive terminal. Approve the host first. The host may only share the selected display for up to 30 seconds, requires ordinary OS screen-recording permission, and stops when the heartbeat, media connection, or deadline fails. Ctrl-C or closing the terminal stops the process. The controller uses a native video window. Screen pixels never traverse Tauri IPC. Diagnostic mode cannot be upgraded to screen mode over the wire. This is a limited development prototype, not signed or installed, not an unattended service, and not ready for Internet deployment. XCap repeated capture at up to 10 fps with letterboxing is a temporary baseline, not a high-performance production capture engine. Real display capture and native video rendering require separate physical-device validation; the test profile never captures a screen.

The desktop supervisor can launch the same trusted lan_view binary with --desktop-session. This mode requires piped stdin/stdout, the same full-key comparison and profile-specific consent, and a retained stdin pipe. EOF or additional input after consent exits the view-only child. The encrypted synthetic CI harness now exercises this mode. Normal real CLI usage continues to require interactive terminals.

## M2.3 routed diagnostic

The separate routed_diagnostic program is WSS-only and always uses synthetic video and an inert backend. It registers with the rendezvous service, requires independently verified peer Ed25519 public keys and explicit test consent, derives the target device ID from its fingerprint, and routes request/decision, exact signed SDP and bounded ICE batches. Both actual video and SCTP DTLS certificates are checked; readiness and fixed simulated input travel directly on the data channel. No screen capture or OS input mode exists in this executable. The server remains loopback-only, and SDP/ICE are not opaque to the signaling server.

The integration harness starts the real local TLS server and two independent diagnostic processes, uses a temporary test-only certificate, verifies decoded video, simulated input release and explicit session closure:
```sh
cargo build -p remote-ex-rendezvous
cargo build --manifest-path crates/media/Cargo.toml --bin routed_diagnostic
python3 scripts/test_routed_diagnostic.py target/debug/remote-ex-rendezvous crates/media/target/debug/routed_diagnostic
``
Requires OpenSSL and the existing GStreamer development dependencies. Run only in an authorized test environment. Not a production remote access client. CI status must be checked before claiming validation.
