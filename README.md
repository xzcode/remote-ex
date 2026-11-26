# Remote EX

跨平台、可私有化部署的远程桌面工具。Remote EX 自主研发客户端、服务端和业务协议，使用成熟底层库，不基于 RustDesk 等完整产品进行 fork。

**当前状态：已发布 [v0.1.0-alpha.1](https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.1)，三平台构建及六项 CI 通过（303163c）。M4 仍有实机验收及原生平台能力缺口。M5 安装环境与打包质量工作并行推进；新代码不等同已发布二进制。**

## 目标

- Windows、macOS、Linux 同一客户端兼任主控与被控，优先实现 Windows/macOS。
- 局域网可独立运行；公网优先 ICE/P2P，失败时通过自建 TURN 中继。
- 不强制账号、不依赖公共服务；显式授权、会话可见、可随时终止。
- 多会话、剪贴板、文件、音频、多显示器、无人值守、语言与主题逐步实现。
- Android/iOS 主控和浏览器端列入 M6，不阻塞桌面首版。

## 开发文档

- [产品需求与平台范围](docs/PRD.md)
- [技术架构与选型决策](docs/ARCHITECTURE.md)
- [会话协议与安全规范](docs/PROTOCOL_SECURITY.md)
- [UI 与交互规范](docs/UI_SPEC.md)
- [里程碑与验收计划](docs/ROADMAP.md)
- [M0 技术验证与依赖审查](docs/TECH_SPIKES.md)
- [开发代理约定](AGENTS.md)
- [M0/M1 第一批交接](docs/M0_M1_HANDOFF.md)
- [M1 第二批交接](docs/M1_DESKTOP_HANDOFF.md)
- [M1 第三批交接](docs/M1_MEDIA_HANDOFF.md)
- [M1 第四批交接](docs/M1_PAIRING_HANDOFF.md)
- [M1 第五批交接](docs/M1_LAN_HANDOFF.md)
- [M1 第六批交接](docs/M1_DESKTOP_LAN_HANDOFF.md)
- [M1 第七批交接](docs/M1_SECURE_VIEW_HANDOFF.md)
- [M1 第八批交接](docs/M1_VIEW_INPUT_HANDOFF.md)
- [M1 第九批交接](docs/M1_CONTROL_HANDOFF.md)
- [M1 第十批交接](docs/M1_CONTROL_GUI_HANDOFF.md)
- [M1 第十一批交接](docs/M1_REVOCATION_HANDOFF.md)
- [M1/M2 第十二批交接](docs/M2_RENDEZVOUS_HANDOFF.md)
- [M2 第十三批交接](docs/M2_PRESENCE_HANDOFF.md)
- [M2 第十四批交接](docs/M2_ROUTED_MEDIA_HANDOFF.md)
- [M2 功能整合交接](docs/M2_FULL_HANDOFF.md)
- [M2 跨网络验收](docs/M2_NETWORK_ACCEPTANCE.md)
- [M3 功能交接与验收清单](docs/M3_FULL_HANDOFF.md)
- [自建部署候选](deploy/README.md)
- [M2.1 信令服务](servers/signaling/README.md)
- [物理设备验收清单](docs/M1_DEVICE_ACCEPTANCE.md)
- [安装、首次使用与排错](docs/INSTALL.md)
- [开发测试包说明](docs/DEV_BUILD.md)
- [发布与验收记录](docs/PREVIEW_RELEASE.md)
- [输入适配说明](crates/input/README.md)
- [LAN 信令诊断](crates/signaling/README.md)
- [原生媒体模块说明](crates/media/README.md)
- [桌面开发说明](apps/desktop/README.md)

## 选型

Rust + Tauri 2 + Vue 3/TypeScript；WebRTC/ICE/STUN/TURN；Rust/Axum 信令服务；coturn 中继；SQLite。GStreamer 是媒体引擎首选候选，待跨平台原生渲染与打包验证后冻结。许可证待所有者决定及第三方依赖审查，公开仓库不自动代表已授予开源许可。

## 安全

仅用于有权限管理的设备。默认不开放无人值守、不绕过系统授权、不隐藏连接状态，不允许未授权键鼠、文件或剪贴板访问。生产凭据不得进入仓库。


## M1 开发包（临时 Actions artifacts）

[Windows x64](https://github.com/xzcode/remote-ex/actions/runs/45577251479/artifacts/15005652383) · [macOS ARM64](https://github.com/xzcode/remote-ex/actions/runs/45577251479/artifacts/15004599699) · [构建与使用说明](docs/DEV_BUILD.md)。两平台完整 CI 通过，源提交 ed2aca9，不含 M2；须登录 GitHub 下载，2026-12-09 到期。需要官方 GStreamer 1.26.11 runtime。Windows unsigned，macOS ad-hoc signed，非正式 Release；真实设备验收未完成。

## M2 开发包（临时 Actions artifacts）

[Windows x64](https://github.com/xzcode/remote-ex/actions/runs/45585068172/artifacts/15008819036) · [macOS ARM64](https://github.com/xzcode/remote-ex/actions/runs/45585068172/artifacts/15008019517)。源提交 bb65000，2026-12-09 到期，外部官方 GStreamer 1.26.11 runtime；非正式发行、不含 M3。真实设备验收未完成。

## M3 第二批开发包（临时 Actions artifacts）

[Windows x64](https://github.com/xzcode/remote-ex/actions/runs/45599629435/artifacts/15015038862) · [macOS ARM64](https://github.com/xzcode/remote-ex/actions/runs/45599629435/artifacts/15015092228)。源 04b1cfd，2026-12-09 到期，需官方 GStreamer 1.26.11 runtime；不含第三/四批新增功能。最新代码需新构建。

## M3 第五批开发包（临时 Actions artifacts）

[Windows x64](https://github.com/xzcode/remote-ex/actions/runs/45616132589/artifacts/15022622948) · [macOS ARM64](https://github.com/xzcode/remote-ex/actions/runs/45616132589/artifacts/15022982319)。构建源 1463dfb，2026-12-09 到期，需外部官方 GStreamer 1.26.11 runtime；macOS 包含已编译签名的 ScreenCaptureKit helper。非正式发行，合成 A/V 测试通过，不代表真实系统音频、次屏输入或音视频同步验收。不包含第六批文件审批变更。


## M3 第六批开发包（临时 Actions artifacts）

[Windows x64](https://github.com/xzcode/remote-ex/actions/runs/45629925477/artifacts/15031092570) · [macOS ARM64](https://github.com/xzcode/remote-ex/actions/runs/45629925477/artifacts/15032035181)。f0046c8，双平台已通过 native package / 审批式文件变更 fixture，2026-12-09 到期；需外部 GStreamer 1.26.11。尚不包含第七批批量下载候选，不代表真实设备验收或正式 Release。

## M3 第七批开发包（临时 Actions artifacts）

[Windows x64](https://github.com/xzcode/remote-ex/actions/runs/45648366767/artifacts/15039712891) · [macOS ARM64](https://github.com/xzcode/remote-ex/actions/runs/45648366767/artifacts/15038784382)。ab61411，完整 CI 成功，含批量下载；2026-12-09 到期，需官方 GStreamer 1.26.11 runtime。非正式发行，不含第八批目录操作。

## M4 development candidate
The current M4 batch adds one-time locally armed trusted-device connection, user-login launch, system tray, Linux X11 read-only view, Wake-on-LAN and platform capability diagnostics. See [M4 handoff](docs/M4_FULL_HANDOFF.md). Actual platform/CI verification is tracked separately; no production release or automatic access is enabled.

M4 second batch adds locally activated, time-limited saved host policies and experimental Linux X11 native input. Linux debug archive build workflow added. Wayland, remote shutdown/restart, virtual display drivers, and physical privacy mode remain pending; no unattended startup access enabled. See [M4 handoff](docs/M4_FULL_HANDOFF.md).

M4 third batch: experimental Wayland Portal read-only sharing (monitor/window/virtual where supported), separately reviewed shutdown/restart requests, and explicitly enabled bounded multi-session host service. No automatic login access or privileged service; real platform validation and native physical display privacy/virtual drivers remain open. See [M4 handoff](docs/M4_FULL_HANDOFF.md).


M4 fourth batch adds experimental native Wayland input and stable portal stream targeting, local view-only sharing privacy pause, active desktop/suspend checks, policy revalidation, and a bounded two-hour visible host service. Physical display privacy and Windows/macOS virtual display drivers remain open. See [M4 handoff](docs/M4_FULL_HANDOFF.md) for exact scope and CI.

## Alpha preview releases

The first preview, [v0.1.0-alpha.1](https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.1), was published on 2026-11-26 from commit `303163ce2e85aea2f00df6f340ec932a6cfb29c0` after all six CI workflows passed. Windows x64, macOS ARM64 and Linux x64 archives and `SHA256SUMS.txt` are available on the release page. Preview builds are published only when all six checks pass on the exact requested master commit and the three archives carry matching commit stamps. These are experimental development archives, not signed installers or production deployments. See [preview release process](docs/PREVIEW_RELEASE.md).

M5 installer/update candidate: see [installation guide](docs/INSTALL.md). New CI candidates build current-user Windows Setup, macOS DMG and Linux DEB plus portable archives; manual update discovery and six additional partial language dictionaries. The published alpha.1 remains unchanged. New artifacts require current workflow success.
