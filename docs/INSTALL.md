# Remote EX — 安装、首次使用及排错 / Install and troubleshoot

## 1. 下载 / Download

[alpha.1 发布页](https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.1) 提供三个平台档案和 SHA256SUMS.txt。只下载当前设备架构对应的档案；保留全部文件，不要只移动主程序。发布页是预发行版，不是已签名或公证的安装器。后续开发包内容以 BUILD_INFO.txt 为准。

校验下载的档案：
- Windows PowerShell: `Get-FileHash -Algorithm SHA256 .\Remote-EX-v0.1.0-alpha.1-Windows-x64.zip`
- macOS: `shasum -a 256 Remote-EX-v0.1.0-alpha.1-macOS-arm64.tar.gz`
- Linux: `sha256sum Remote-EX-v0.1.0-alpha.1-Linux-x64.tar.gz`

Compare the result with the matching SHA256SUMS.txt entry. A checksum detects file corruption; it is not an independent publisher signature.

## 2. 安装运行库 / Runtime prerequisites

| Platform | Install and launch |
| --- | --- |
| Windows x64 | WebView2 Runtime and official GStreamer MSVC x86_64 runtime (1.26.11 was used for alpha.1). Extract the ZIP and run remote-ex-desktop.exe beside its sidecars. Restart the app after installing runtime; for alpha.1 ensure the official runtime bin directory is on PATH. |
| macOS ARM64 | Install the official GStreamer 1.26.11 framework, extract the archive and open Remote EX.app. Ad-hoc signature only, not notarized; obey Gatekeeper and ordinary Screen Recording / Accessibility permissions. Do not disable system security. |
| Linux x64 | Use a compatible distribution with WebKitGTK 4.1, GStreamer 1.22+ with WebRTC/VP8/ICE/Opus plugins, and its native desktop dependencies. Extract the archive and run remote-ex-desktop from the extracted directory. X11 needs a normal local X11 session; Wayland requires the installed ScreenCast/RemoteDesktop portal backend and PipeWire plugin. |

Official runtime: https://gstreamer.freedesktop.org/download/ . For exact Ubuntu build prerequisites consult the Linux CI workflow. Do not use unrelated DLL/plugin downloads. Development-package ABI compatibility is not universal across Linux distributions. The application does not auto-install libraries or ask for root.

## 3. 本机诊断（新开发包） / Local runtime check (new development package)

Open Diagnostics → Installation and media runtime → Check local runtime. This checks fixed neighboring media component filenames and the package's optional sidecars.json manifest, reports installed GStreamer version and known missing elements, and runs a tiny generated 160×90 VP8 encode/decode pipeline. No real display, audio, identity key, network request or permission prompt is used. The runtime report may be copied manually; it contains version, architecture, component names, status and plugin names, not paths, keys, connection addresses, screen contents or logs. It is not an OS permission or two-device test.

- Missing/mismatched component: re-extract the matching archive without mixing versions. With an existing invalid manifest or component mismatch, launch is refused. Old packages/development trees without a manifest are marked unverified, not considered publisher-authenticated.
- Media component cannot start: install the official matching runtime, check architecture and PATH, and restart the app.
- Missing media elements: install the matching distribution/official runtime plugin packages. Some listed optional elements are platform-specific.
- Local runtime check passes, remote connection fails: verify peer keys, exact selected profile, server enrollment, ordinary OS permission, and the independently configured WSS endpoint. No public relay is provided by this release.

## 4. Local or self-hosted connection

Use two explicitly authorized devices. For initial LAN testing, use the secure test profile and follow the full local key comparison and consent steps in DEV_BUILD.md. Use routed view/control only with an administrator-configured, trusted WSS rendezvous and the matching server ID. See deploy/README.md and docs/M2_NETWORK_ACCEPTANCE.md. No automatic public listener, default unattended access, permission bypass, or auto-deployment is enabled.

Actual screen, input, sound, lock/suspend, Wayland virtual display and power behavior still need authorized physical-device testing. Linux real screen viewing uses the routed native sidecar, not the LAN synthetic diagnostic. Keep the host visible and use Disconnect; do not rely on this alpha for critical remote access.


## 5. Installation packages (next development build)

These candidates are not retroactively included in alpha.1 and are not yet released. Use the matching development artifacts only after the current workflow passes.

| Platform | New package | Scope and removal |
| --- | --- | --- |
| Windows x64 | Setup.exe (NSIS) | Installs into a fixed, version/commit-specific directory under the current user's LocalAppData Programs folder. Start-menu shortcut and current-user uninstall entry. No service, elevation or automatic launch. Disable login startup in Remote EX and disconnect/close before uninstall. The uninstaller removes only its known application files, preserving credentials and user data. |
| macOS ARM64 | DMG | Drag Remote EX.app to Applications; close the app and disable login launch before removal. External GStreamer framework required. Ad-hoc signature, not notarized. |
| Linux x64 | DEB | On a compatible build-distribution system, inspect with `dpkg-deb --info <file.deb>`, then explicitly use your package manager, e.g. `sudo apt install ./<file.deb>`. Files go under /opt/remote-ex with an application menu entry; package manager handles declared dependencies. No service or privileged listener is installed. Actual distro installation and GUI launch remain unverified. |

All formats preserve the prior portable archives. Windows/macOS do not silently download or install GStreamer/WebView2; Diagnostics provides fixed official help links. Linux uses package metadata to declare linked libraries and GStreamer plugins. The DEB is not guaranteed compatible with other distributions or older glibc. Always close sessions before upgrading or uninstalling.

## 6. Update discovery and locales

Settings → Version and updates shows compiled tag/commit and manually checks the official GitHub release API with a bounded HTTPS request. Stable-only and preview-inclusive filters are available. Development builds without an exact published tag are not falsely marked outdated. Only exact platform asset names are listed; use the official releases button, verify SHA256SUMS and install manually. No automatic download, binary replacement, background update check or credential is used.

English and simplified Chinese remain the complete existing application dictionaries. German, French, Spanish, Portuguese, Japanese and Korean are partial additional dictionaries with explicit message counts and English fallback for untranslated instructions/consent text. Native sidecar/viewer, OS dialogs and other hard-coded messages are not fully localized. System language selection respects browser/OS language preferences; unsupported locales fall back to English.

New packaged builds embed their exact source commit and refuse missing or mismatching sidecar manifests; source builds without an embedded commit may remain unverified. DEPENDENCIES.json records resolved build inputs from Rust and frontend lockfiles, including dev/target-specific entries; it is not a full SBOM or licensing/compliance audit.


## 7. Saved server configurations and exact release metadata

On Server connection or Device ID connection, expand Saved servers, enter the WSS address and independently verified complete server ID, name the configuration and explicitly save. Select a saved server and click Use this configuration to fill the fields. No connection starts. CA paths, credentials, peer keys and authorization are not stored in server profiles. Removing one profile only removes the address book entry, not previously verified device trust; revoke device pins or policies separately. Profiles cannot silently overwrite existing names, URLs or IDs.

The update panel now shows a validated SHA-256 digest if GitHub includes it in official release metadata and can open the exact release page. Check the downloaded file against the published checksum. This remains manual installation, not cryptographic publisher-signature verification or an automatic updater.

For self-hosting, see [deployment instructions](../deploy/README.md): use the new server `--check-config` option before starting it, or build the optional non-root signaling container. These are development candidates; no server is deployed by the app.


## 8. WSS preflight, downloaded-file check and native viewer

Before registering, click Check WSS connection on the server or routed connection page. The exact entered endpoint must pass ordinary TLS certificate/hostname validation and return the independently configured server ID. No device identity is sent and no registration occurs. Routed mode can use a certificate-only private CA file; this is not a TURN, enrollment, media or performance test. Time shown is elapsed handshake time, not network RTT. Do not use a result for an endpoint whose address/ID/CA has changed.

In Version and updates, explicitly check releases first, then choose Verify downloaded file next to an asset with an official SHA-256 digest. Select the exact matching filename in the native file dialog. The application compares bounded file size and hash against recently fetched official metadata without unpacking or running the file. No private file path or content is uploaded. Matching digest does not establish publisher signature. Linux dialog requires a functional XDG file chooser portal; it does not imply Linux content-exchange modes are supported.

New native routed view/control windows wrap local toolbar buttons when narrow, preserving the separate picture/input rectangle. Desktop language is propagated to new viewer processes. European language toolbar labels use compact bitmap-safe ASCII; Chinese/Japanese/Korean use English bitmap labels with localized native window title/legend until CJK text rendering is implemented. Legacy LAN viewer, native errors and OS dialogs remain partially localized.


## 9. Complete UI dictionaries, native fonts and exact CA scope

The eight application dictionaries contain all 423 current keys. A dictionary coverage check runs in desktop CI; translation accuracy, native dialogs, OS errors and legacy LAN viewer still need review. Routed viewer renders its fixed translated toolbar labels with installed fonts in a background worker; a cached bitmap fallback remains available if fonts are missing. Install an appropriate system CJK font through your normal OS package manager if Chinese, Japanese or Korean labels remain in English. No fonts are included or downloaded by Remote EX. Native viewer acceptance is still pending.

Private CA selection is now consistent across server registration, WSS preflight, routed sessions and registration CLI: an explicitly selected certificate-only PEM replaces public trust roots for that connection. Use an absolute local path, no symlink/private key, at most 256 KiB and 32 certificates; hostname and validity checks remain mandatory. No fallback to public roots after a custom CA validation failure. Server profiles do not save CA paths.

Package DEPENDENCIES.json adds declared-license evidence and triage flags; see [third-party review](THIRD_PARTY_REVIEW.md). External GStreamer and WebView2 setup still require the official runtime.
