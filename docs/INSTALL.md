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
