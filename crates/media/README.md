# Native media engine (M1 development)

Independent Rust crate using GStreamer 1.20+ / gstreamer-rs 0.23. VP8/RTP, WebRTC DTLS-SRTP/ICE, sender and receiver, bounded ICE event queues, SDP offer/answer primitives, native RGBA appsrc input, native decoded sample output. No application-level identity, permission integration, STUN/TURN configuration, screen streaming, OS input, renderer, or external listener yet. Do not expose this prototype as a remote control service.

The only executable uses two real WebRTC endpoints in a single process and an internally generated moving pattern. It checks successful peer states and at least 30 decoded frames. It does not measure real screen-to-screen latency. The media pipeline consumes native GStreamer buffers; no frame-by-frame Tauri IPC.

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
