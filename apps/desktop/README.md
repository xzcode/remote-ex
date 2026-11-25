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
