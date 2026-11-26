# Remote EX Web compatibility lab (M6)

**Synthetic-only local development candidate, not a remote desktop client.**
Two browser peer connections in one page, one explicitly authorized run lasting at most 30 seconds. No remote endpoint field, signaling server, registration, screen capture, microphone/camera, input, clipboard, file transfer, background session, STUN/TURN, or persistent identity. Browser-generated host ICE candidates are exchanged only between the two local peer connections. Do not publish this lab as a real remote-access service.

## Build and use

From the repository root, with a current Rust toolchain and Node.js 22:
```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.106 --locked
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build --release --manifest-path apps/web/crypto/Cargo.toml --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir apps/web/pkg --out-name noise apps/web/crypto/target/wasm32-unknown-unknown/release/remote_ex_web_crypto.wasm
node apps/web/serve.mjs 8080
```
Open http://127.0.0.1:8080 locally in a modern browser with Ed25519 WebCrypto, WebRTC, canvas capture and certificate statistics. The local server binds numeric IPv4 loopback only and serves a fixed list of assets with restrictive CSP and Permissions-Policy. A generated CI bundle can be extracted and started with `node serve.mjs 8080` in its directory. No external public preview is configured. Unavailable browser primitives or failed verification stop the test.

1. Check the synthetic-only consent and click Start local test.
2. Verify the six real checks and actual decoded frame count. Input is not captured or sent.
3. Disconnect, leave the page, hide the tab or reach the 30-second bound to stop tracks and close peer connections.

## Evidence and limitations

Rust WASM uses the existing RouteBinding verifier, shared profile 0 and Noise pattern via Snow 0.10.0. Browser WebCrypto verifies public Rust vectors, creates ephemeral non-extractable Ed25519 private keys, signs actual mutually negotiated Noise and SDP transcript data, and rejects changed proof/replayed record. WASM accepts only the fixture proof and fixed test record, never arbitrary application payloads. The local WebRTC offer/answer is signed exactly, and the connected transport's certificate fingerprints are compared to the signed SDP before a generated canvas track is attached. This is not an authenticated external WSS/Noise signaling bridge. The two peers are controlled by the same local lab; there is no independent remote consent or trust ceremony. No desktop/GStreamer interoperability is established. A real browser route owner, server enrollment, network signaling, secure storage, session revocation, independent peer verification, real media and authorized control remain separate gates.

CI is designed to run native Rust unit tests and real headless Chromium with no capture permissions or external HTTP requests. Tests are not physical-device or production-browser acceptance. New Rust bindings (wasm-bindgen 0.2.106), Snow, getrandom JS backends and Playwright 1.56.1 are development dependencies; review actual resolved versions and redistribution obligations before shipping. The browser laboratory artifact is separate from published alpha.2.

## WSS compatibility fixture (CI only)

The next lab workflow is pinned to Ubuntu 24.04 because Playwright 1.56.1 cannot install Chromium on the Ubuntu 26.04 hosted image. Native Rust tests and wasm32 generation passed on the earlier runner, but Chromium never ran there. The new CI additionally builds the existing rendezvous binary and uses a disposable loopback TLS service with a generated CA imported only into a temporary browser HOME/NSS database. It checks the exact configured browser Origin, independently selected server ID, two generated enrolled identities, signed registration and v1 route, mutual Noise authentication, encrypted host offer/controller answer and ordered mutual SDP signatures, actual DTLS fingerprints and generated video decoding. No certificate verification bypass or external relay is used. The WSS fixture module is enabled only by the test harness and is not included in the downloadable laboratory bundle. Its browser and Rust peers are still synthetic; it is not a real desktop controller or a deployed Web app. CI result pending.
