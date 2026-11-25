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
