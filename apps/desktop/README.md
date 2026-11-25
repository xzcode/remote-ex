# Remote EX desktop (M1 development build)

This is a local screen-capture diagnostic, not a remote-control client yet.
The Vue app uses Tauri 2, with xcap 0.9.10 (Apache-2.0 candidate; full transitive audit pending) for monitor enumeration and one-shot capture. This selection does NOT replace GStreamer as the continuous media candidate.

## Build
Install Rust stable and Node 22 LTS, plus Tauri platform prerequisites. Linux additionally needs xcap build prerequisites (pkg-config, clang, XCB, Xrandr, DBus, PipeWire, Wayland, EGL development packages). See:
- https://v2.tauri.app/start/prerequisites/
- https://github.com/nashaofu/xcap

```sh
cd apps/desktop
npm install
npm run tauri dev
```

For compile-only checks: npm run build, then cargo check --manifest-path src-tauri/Cargo.toml. Bundling is deliberately disabled in this development version; no installer or signed release is claimed. Cargo and npm lockfiles still need to be committed after a verified dependency resolution.

## Local manual check
1. Launch on a logged-in desktop. No capture, listener, device registration or remote input runs at startup.
2. Select Local diagnostics; check permission. On macOS explicitly request screen recording permission and grant it via system settings; restart if the OS requires it. Windows and Linux respect capture restrictions of the current OS/session.
3. Refresh displays; choose an available display; click Capture once. Actual pixels are returned as a bounded PNG for a local preview (max 1280×720), not a synthetic placeholder. Image dimensions reflect the preview, not the original screen.
4. Clear preview or navigate away; the UI revokes the temporary Blob URL. No screenshot files are written by application code; no remote transmission. Backend/library internals and actual OS permission behavior need platform validation.
5. Verify invalid/disconnected display and denied permission errors, no background capture, and language/theme persistence.

Headless runners can compile/test, but cannot establish that a real display or permissions work. Wayland and secure desktops are not claimed supported. The temporary square icon is for development builds only.

## LAN diagnostic in the desktop UI

The development desktop supervises the exact native `lan_diagnostic` executable next to its own executable (on Windows, `lan_diagnostic.exe`). Build it on the same platform with GStreamer 1.22+ using the instructions in `crates/media/README.md`, then copy it alongside the Tauri development binary (typically `apps/desktop/src-tauri/target/debug/`). Install required native GStreamer runtime libraries and plugins. No media binary is bundled by default; if it is absent, Prepare reports that requirement instead of pretending to connect.

Choose host/controller and a specific private IP:port (or loopback), prepare each application, independently verify the two ephemeral public keys, and explicitly approve the diagnostic (host first). The controller may select a native synthetic-video preview window; the diagnostic exchanges only generated test media and a fixed simulated input sequence. The UI shows actual process milestones and the outcome, and provides Disconnect. It never launches automatically, never reads real desktop pixels in this session and never executes OS input. The desktop supervisor terminates its direct child on disconnect, expiry or window/application exit. Unencrypted diagnostic signaling is unsuitable for production use.

The sidecar is a development-only trusted local build, not signed/packaged yet. Do not substitute untrusted binaries. Native preview and actual two-machine behavior still need physical-machine verification.

## Screen sharing UI (development)
Build the separate trusted lan_view executable with local-screen enabled and place it beside the desktop executable. Choose Screen sharing (view only), host/controller, explicit address; host refreshes and selects a display. Verify the full 128-hex bundle and explicitly confirm on both sides. The UI supervises the fixed sidecar, keeps the consent pipe open, displays active status, and terminates it on disconnect or application exit. The view process also exits on loss of the supervisor pipe. The encrypted test profile is distinct and never captures the screen. OS permission for the separate capture executable may be required. Only view access; no actual system input. Unpackaged, unverified on physical Windows/macOS desktops, limited to 30 seconds. Do not expose to the Internet.

## M2 registration preview
The Server connection page supports explicit WSS registration with a verified certificate/hostname and administrator-provisioned public server ID, or explicit numeric IPv4 loopback WS development. Temporary identity is default; the unchecked opt-in box enables the OS credential store for persistent identity on Windows/macOS only. No startup keychain access. All incoming requests are declined, heartbeat timeout/offline are surfaced, no automatic reconnect. Registration runs up to five minutes, ends on user stop or application exit; no public remote desktop session integration yet. Custom CA files are supported by the registration CLI, not this GUI. See servers/signaling/README.md.
