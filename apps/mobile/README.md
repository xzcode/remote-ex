# Mobile controller candidate

Android reuses the existing browser WebCrypto/WASM/VP8 path in a private packaged WebView origin. iOS now has a Swift/UIKit WKWebView shell candidate for a separately reviewed HTTPS controller site; it is not a native WebRTC client or signed iPhone release.

## Build Android
Use JDK 17, Gradle 8.9, Android SDK platform 35, build-tools 35.0.0, Node 22 and the existing WASM build commands in apps/web/README.md. From repository root, build WASM and then:
- Create the staging parent with mkdir -p build.
- Run node scripts/stage-mobile-client.mjs build/mobile-assets for an offline capability/identity build. To enable a server, pass a separately reviewed operator JSON (see operator-config.json.example) as the final argument. Use a fresh staging directory when changing configuration. Never put private keys, passwords or temporary credentials in the file.
- From apps/mobile/android run gradle --no-daemon :app:assembleDebug.
- Install the debug APK on an authorized Android device for testing. The declared minimum is Android 8, not a compatibility promise. A current WebView must support non-extractable Ed25519, WASM, WebRTC, IndexedDB and connected DTLS stats.

The app intercepts fixed bundled files under https://appassets.androidplatform.net with restrictive headers. No JS bridge, file/content access, external navigation, mixed content, certificate-error bypass, local capture or background service. Android declares INTERNET only. Backgrounding destroys WebView sessions; resuming returns to the mobile landing. Origin-local IndexedDB identity persists, but is not a hardware-backed Android Keystore identity. Backup is disabled. No default server. Enroll independently verified device ID and key, with TLS and exact REMOTE_EX_WEB_ORIGIN=https://appassets.androidplatform.net. New M6 server candidates support a bounded explicit list via REMOTE_EX_WEB_ORIGINS; older servers require one configured origin. Never enable wildcard or bypass TLS.

## Touch UI
With independently approved profile-2 permission and decoded media, select mouse/keyboard or touch click, drag, right click or scroll, then enable input. A single finger within the authorized display geometry is accepted. Multi-touch, focus loss, cancellation or out-of-bounds dragging release and disable input. The native dispatcher independently cleans up on transport loss. No text/IME injection, background input, clipboard, file transfer or mobile host mode.

## iOS and Android browsers
Stage the same assets into a separate clean directory using the approved operator config. Serve only those files under a trusted HTTPS origin with the exact headers.json response headers, correct application/wasm MIME type and the matching signaling Web Origin. Do not expose the localhost diagnostic server as a proxy. The Safari/iOS browser must separately pass capability checks, persistent identity reload, actual VP8/WebRTC and DTLS statistics, and touch lifecycle. A Swift/UIKit shell now exists (see below); native WebRTC and hardware-backed keys are not implemented.

## Acceptance
CI builds a debug APK with no endpoint and checks packaged assets. Actual Android install, WebView capability, identity persistence, background/relaunch, authorized media and input, and cross-network relay require device tests. No public deployment or new release.

## iOS app shell (source candidate)

The Xcode project targets iOS 17 or later. It opens only a build-time reviewed HTTPS DNS origin and fixed controller pages; WKAppBoundDomains is bound to that host. Default Info.plist is offline. To stage a configured plist without altering the repository template, run `python3 scripts/configure-ios-controller.py /path/to/new/Info.plist https://your-reviewed-mobile-site.example.net` (parent must already exist), then override `INFOPLIST_FILE` to that absolute path during Xcode build. Host the staged HTTPS assets with their reviewed response headers and matching signaling Origin. No arbitrary site picker, ATS exceptions, JS bridge, media capture permission, background session, or automatic reconnect. System TLS trust remains enabled. The web origin's IndexedDB keys are not hardware-backed Keychain keys. The app discards WKWebView on foreground loss or explicit disconnect. CI builds an unsigned iOS simulator app only; actual WKWebView support, real-device installation, signing, physical control and distribution remain unverified.

## Extended sessions and saved details

The optional operator config `extended: true` allows a separately selected and approved 300-second foreground session with existing native profile 22/23 and a bilateral 420-second routing lease. Default is 30 seconds; no auto-reconnect or lease renewal. Runtime quality uses the native Auto/Full/Balanced/Low protocol. Input is deliberately disabled at reduced decoder resolution and released on a quality change; reactivate explicitly at full size. Saved connections contain only a name and exact server/host details in origin-local storage, never a session grant, key secret, credential, mode or reconnect setting. Loading a saved entry resets control, duration and consent. Run `node scripts/verify-mobile-stage.mjs build/mobile-assets` before staging assets for use.

## Runtime acceptance (CI only)

The mobile workflow also builds a separate Android instrumentation APK and installs the offline client on a fresh API 36 emulator to check actual WebView secure context/capabilities, origin-local identity lifecycle and background destruction. The iOS job attempts actual simulator install and launch/relaunch of the offline app. These steps run after build artifacts are uploaded; an unavailable emulator, missing required API or failed probe is a test failure, not a compatibility pass. No production APK test code or signing changes. Actual authorized physical remote sessions remain separate; see docs/M6_MOBILE_RUNTIME_ACCEPTANCE.md.

### Capability diagnostics

The landing page now reports per-capability reason codes: secure context, Ed25519 proof/non-exportability, bounded WASM binary initialization, transactional IndexedDB temporary key clone, actual local VP8/data-channel offer and stats, and pointer capture. The checks do not set a local description, gather ICE, open signaling or enroll an identity. A missing API disables the connection. Android instrumentation logs the provider version and fixed reason codes only. The previous API 36 emulator installed the APK successfully but failed the original undifferentiated capability gate; the precise cause is still awaiting the expanded runtime check. Do not bypass TLS, substitute a weaker key, or claim physical support from an emulator build.

### Known Android WebView requirement

Actual API 36 runtime with Android System WebView 133.0.6943.137 failed only the required WebCrypto Ed25519 operation (NotSupportedError), while secure context, WASM, generic IndexedDB, WebRTC and pointer checks passed. Chromium 137 introduced stable Ed25519 support; see https://developer.chrome.com/docs/modern-web-guidance/guides/security/digital-signatures. Update Android System WebView / Chrome using the official device update channel and restart the app. Version 137+ is a useful baseline, not a compatibility guarantee; actual sign/verify/non-exportable key storage and media checks are mandatory. The app does not implement an alternate signature or exportable-key fallback. Identity creation is explicitly disabled before user action when the capability is unavailable. No automatic package installation, browser flags, arbitrary URL navigation or security bypass. The CI matrix distinguishes API 36 known legacy refusal from a strict supported API 37 candidate; a legacy refusal is not Android controller availability. A newer image's actual provider and complete runtime remain to be checked.
