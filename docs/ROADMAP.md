# 开发路线与验收

2026-11-25。状态词：设计完成 / 验证待办 / 开发中 / 实现待验 / 已验证 / 不支持。不得以文档或编译代替真实功能验收。

| 阶段 | 工作包 | 当前状态 | 退出条件 |
|---|---|---|---|
| M0 | 需求、架构、UI、协议安全、依赖选型和 spike | 文档基线已建立，合成 VP8、插件检测、同/跨进程合成媒体与数据通道通过；加密信令与实际 DTLS 双进程测试通过；真实采集与窗口待实机验证 | 媒体/渲染/输入/构建路径验证，版本和依赖清单冻结 |
| M1 | 桌面骨架、局域网采集/编码/传输/解码/输入 | 开发中：核心、桌面 CI、同进程 WebRTC 和 RGBA 回环通过；LAN 信令与独立进程媒体/数据通道集成通过；桌面 LAN 会话管理与加密只读 CLI 集成 CI 通过；只读 GUI 与原生输入适配 CI 已通过；加密控制模拟输入集成测试通过；桌面实验控制、正常撤销和独立采集线程已编写，增量桌面 CI、双向撤销测试通过；Windows x64 / macOS ARM64 开发包均通过完整 CI 并生成；真实设备验收未完成 | 两台真实设备端到端授权控制；权限拒绝和断开释放验证 |
| M2 | 信令、设备 ID、认证、ICE、TURN、重连 | 实现待验：M2.1/2 本地信令/WSS 通过历史 CI；本批完成端到端 Noise 路由、可信设备/上线、原生 view/control UI、ICE/STUN/TURN、短期凭据、TLS 准入配置与重新注册代码；最新完整媒体/桌面/native-build、物理跨网及生产评审待验证 | 两个网络下直连与强制中继测试，身份、权限与撤销安全检查 |
| M3 | 设备、音频、多屏、文件、剪贴板、统计、多会话 | 功能候选：10个工作包均有代码，部分子功能未完成；最新自动化与实机验证待办，详见 M3_FULL_HANDOFF.md | 功能端到端操作和权限测试 |
| M4 | 无人值守、后台服务、Linux、高级能力 | 四批实现候选；alpha.1 自动验收通过，物理设备与未支持平台能力待验 | 分平台验证权限、服务生命周期和明确限制 |
| M5 | 多语言/主题、安装包、更新、安全与部署、Release | alpha.1 已发布；八语言字典、安装器、手动更新校验、部署候选已实现；alpha.2 已发布，七项 CI 通过；正式签名与实机验收未完 | 正式发布清单及三平台验证；未支持能力明示 |
| M6 | Web、移动主控、管理端及多中继 | M6.1–3 第一批六项 CI 已过；M6.4/5 浏览器 WASM/合成媒体实验已编码，待浏览器 CI；真实 Web/移动主控未实现 | 各平台独立验收 |

## M0 执行顺序
M0.1 产品文档与模块边界；M0.2 媒体引擎和原生渲染 spike；M0.3 平台采集/输入权限 spike；M0.4 构建打包及依赖许可证审查；M0.5 ADR、协议 schema 与版本锁定。

## M1 执行顺序
M1.1 Cargo workspace + Tauri/Vue 框架，语言主题基础；M1.2 平台捕获和本地真实预览；M1.3 采集到编码及解码渲染链路；M1.4 本地信令、身份配对与 WebRTC；M1.5 授权键鼠与坐标映射；M1.6 真实设备控制及故障清理。先完成一个 OS 的真实双端闭环，再验证跨 OS。

## 验证约定
每个里程碑只跑与功能、协议兼容、构建和安全相关的必要测试。记录 OS/版本、构建 SHA、操作步骤、实际结果、未验证项；未获得真实目标设备时标为待验，不能宣传性能目标为实测。每批次留下可继续工作的交接记录，不自动发布或部署未经验证的构建。

## M2 执行顺序
M2.1 本地 rendezvous：挑战认证、设备 ID、会话请求/拒绝/授予、严格路由与超时（本地测试通过）。M2.2 持久设备身份、WSS 服务/客户端注册、配置与在线管理（代码已编写，TLS 与新 GUI CI 待结果，持久存储待实机；可信对端管理仍待）。M2.3 将现有 Noise/Ed25519/DTLS 验证会话与服务端路由集成，完成合成端到端跨进程测试；之后才开放真实授权会话。M2.4 STUN/ICE 直连、TURN 短期凭据、受控中继、部署和限流策略。M2.5 两个网络下直连/中继、断线重连、撤销与真实设备验收。M2 并行开发不代表 M1 物理设备退出门槛已满足。

### M2 功能整合状态（第十五批）

- M2.1：TLS WSS、注册挑战、设备指纹路由、会话归属/有效期、限额、设备准入名单、明确 host grant。默认本地；非回环绑定必须显式 TLS + 设备名单 + 固定端口，尚未部署。
- M2.2：独立可信公钥、SQLite pin book、显式持久身份、上线前获得设备 ID、心跳/下线、可选重新注册。OS 凭据实机及新 GUI 待验。
- M2.3：设备 ID 对应完整 key pin、独立 profile/grant、Noise NN 握手 hash 绑定 Ed25519 双方签名、端到端 sealed SDP/Proof/ICE、实际 video/SCTP DTLS、独立原生 view/control 和 synthetic 诊断、显式本地撤销。代码候选，最新媒体与原生构建尚待复验。
- M2.4：operator ICE config，STUN/ICE All 策略与 TURN fallback、relay-only（双方候选必须是 relay）、coturn REST 临时凭据与部署模板；实际强制中继 CI 待结果。
- M2.5：仅在线注册有限重连（2/4/8秒，最多3次，显式开启），任何新媒体会话需重新请求和批准，不自动恢复输入授权。虚拟/物理跨网与失效恢复验收尚未完成。

不要把实现候选称为阶段完成。详见 M2_FULL_HANDOFF.md 和 M2_NETWORK_ACCEPTANCE.md。

## M3 执行记录

M3.1 地址簿、在线状态和历史；M3.2 邀请码与验证；M3.3 原生窗口缩放/输入映射；M3.4 画质预设与条件自适应；M3.5 被控端显示器选择；M3.6 显式授权双向单次剪贴板；M3.7 显式选定单文件收发、校验、进度、当前会话暂停/继续；M3.8 独立音频；M3.9 三个独立主控会话槽位；M3.10 统计。上述不是十项完整实机 PASS，退出条件与剩余缺口见 [M3 交接](M3_FULL_HANDOFF.md)。仍需 M1/M2 物理设备安全验收。禁止未经授权启用无人值守或发布。

### M3 第二轮收尾

- f468122：Windows/macOS 桌面 CI、Linux 双向文件/暂停/继续/完整性、双 NAT/relay 回归成功。独立合成音频 harness 缺监督参数；开发包 Windows 暂停管道 CRLF 被拒绝，macOS 暂停/恢复成功后音频 harness 失败；旧 M3 包未上传。
- a9576f0 / b833e9d / af1076f / d41fe73 / 88b5c1f / fe04f85：修复测试入口、独立 Winit + Softbuffer 全屏及本地工具栏、只读预选多屏切换和拼接（最多四块）、真实首帧后 readiness、固定控制行 LF/CRLF 与本地实时音量、坐标回归、各槽位独立稳定身份预登记。#45599302773 核心成功，最新整批 CI 与新开发包待验证。
- 剩余：原生窗口与多屏实机、非主屏控制、同一身份真正会话复用、自动剪贴板、远程目录/拖拽/跨连接断点续传、macOS system audio capture 和音视频同步、动态分辨率/持续控制、性能统计精度。独立槽位身份是当前部署的可用方案，不等于同一 ID 并行；保留既有实机退出门槛。

### M3 第三轮收尾

- fe04f85 的 Core、Desktop checks、Native media engine 完整通过；音频 synthetic test 修复、双 NAT 与文件暂停恢复回归通过。04b1cfd 两平台完整 native Release 编译完成，开发包运行检查待结果。
- c98fc96 / 25f8aca / 4dc018d：新增本地显式 checkpoint、原始双方设备 key 与完整 offer 绑定、前缀 SHA-256/偏移双端确认、最终完整 SHA-256、新目的文件禁止覆盖、断点 UI。
- 135c23f / 381b42c：独立目录 profile，明确选择本机源目录与新目的目录，最多256条/8层/64 MiB，完整 manifest 检查、拒绝链接/reparse points、特殊文件/重复路径/跨目录/覆盖；本地拖拽仅选择路径。
- 5bf6f46：独立明确授权的双向文本剪贴板同步，30秒/8KiB，初始基准不发送，检测更新与回声抑制，不读取图片或记录内容。
- b35a17e：真实中断 checkpoint 保存、预置相同 key/offer 断点续传、目录包收发与 synthetic 双向剪贴板测试。Core #45600808482 成功；最新增量 CI 待结果，旧 native build 不取消、不假定包含第三批。
- 剩余同一身份的真正会话复用、非主屏控制、完整远程目录浏览/管理、跨连接真实双端同身份中断再连实测、合并音视频、macOS 系统声音采集、长期会话与真实设备验收。详见 M3_FULL_HANDOFF.md。不得以功能候选或合成测试替代真实 OS 验收。

### M3 第四轮收尾

- 第三批核心、媒体（实际断点、目录、synthetic clipboard）、两平台 desktop CI 已通过；第二批 Windows/macOS 完整开发包均上传。第四批新增独立域签名的同 ID 主控注册，精确 socket session ownership，最多三条并行主控，GUI 显式共享原设备身份；新增只读远程文件列表、授权根及按 index 下载一文件，仍无任意远程文件操作。最新核心/媒体/desktop/native build 待复验，完整同 ID 并行画面/实机待。详细证据见 M3_FULL_HANDOFF.md。
- 下一步：交付最新 M3 包、音视频同会话、macOS system audio、剩余文件管理与非主屏控制等实际功能/验收；不提前标记 M3 完成。

### M3 第五轮收尾

- 第四批 81e56b6 核心/媒体/双平台桌面 CI 通过；完整原生包 #45614297499 尚在执行。新增 macOS 13+ ScreenCaptureKit 系统声音采集候选、VP8+Opus RTP+可靠输入同会话、明确签名音频权限及 GStreamer 共享时钟/播放音量；Windows 单一选定次屏实验控制与 DPI/实际光标/前台窗口检查。最新核心、媒体、桌面、原生构建 #45616132615/#45616132618/#45616132613/#45616132589 待结果。详见 M3_FULL_HANDOFF.md。物理 A/V 同步、真实 macOS 系统音频、Windows 非主屏 DPI、macOS 非主屏、完整文件写入/管理、长时间真实会话等仍待，不标记 M3 完成。

### M3 第六轮收尾

第五批 1463dfb 完整核心、媒体、双平台桌面、原生输入与开发包通过；VP8 30帧 / Opus 169 decoded buffers / inert release。新增 profile19 远程文件变更双重授权、目录句柄限定操作、最多四项、新建空文件/目录、普通文件改名/移入恢复目录、完整 review/回执。核心测试通过，1c7fb95 最新端到端/桌面、两平台完整构建待结果。非完整远程文件系统或物理设备验收；未完成 macOS 非主屏、动态分辨率与长期会话。见 M3_FULL_HANDOFF.md。


### M3 第七轮
第六批 f0046c8 开发包和 1c7fb95 核心/媒体/桌面 CI 已通过。新增独立 profile20 批量下载候选：最多4项、相对目录结构、SHA-256、准确选择清单与新目的目录，不覆盖；单文件 profile15 与审批 profile19 不变。最新 CI/实机尚待验证，仍未进入 M4。

### M3 第八轮
第七批 ab61411 核心、媒体、原生输入、桌面和双平台开发包均成功。第八批候选在原有逐批本机审批下扩展文件/目录重命名、移入恢复目录、从恢复目录选定项目恢复到明确新路径。目录清单、内容指纹、子树大小、路径冲突、同卷独占移动；不覆盖，不永久删除，不自动清空恢复目录。新增平台单元测试和加密端到端 fixture，最新 CI 待结果。仍需 macOS 次屏控制、动态分辨率及长期/真实设备验收。


### M3 第九轮
fea12c7 的四类 CI 因核心 unsafe lint 与 Vue 模板语法失败，已作针对性修复；新增独立 macOS Quartz 单屏实验输入、全局坐标和 AX 前台窗口检查，Retina 等比捕获，原生构建及物理设备待验证。保留 Windows 路径、默认主屏、明确本机许可和短时原型限制。动态分辨率、长期连接仍待。


### M3 第十轮
第九批 core/media 自动化成功，双平台 native build 待最终结果。扩展签名会话、双方一次性路由租约、WSS 心跳、中继凭据、真实 encoder caps 动态画质、接收固定画布、手动/自适应策略及端到端测试候选已实现，默认30秒、扩展5分钟；新 CI/物理设备待验。不得标为 M3 实机验收完成。下一阶段可并行设计 M4，但不启用无人值守或绕过 OS 授权。

### M4 第一整批
M4.1/M4.2：一次限时预授权等待（已保存设备公钥、屏幕限定权限、五分钟、可见应用、单次连接），撤销信任/断开/退出停止；不是跨重启永久无人值守。M4.3/M4.4：本机显式用户登录启动与系统托盘，启动默认离线，不安装高权限服务。M4.5：独立 Linux X11 read-only sidecar，桌面与 Xvfb CI 候选。M4.6：检测 Wayland 并拒绝 XWayland fallback；真正 Portal 媒体/输入未实现。M4.7：局域网 Wake-on-LAN，其他远程电源操作未开放。M4.8/M4.9：能力界面明确不可用，不安装驱动、不关闭物理屏幕。M4.10：新测试及真实平台验收清单见 M4_FULL_HANDOFF.md；本批 CI 未完成，不能宣称 M4 已验收。

### M4 second integrated batch
Saved expiring per-device host policy with exact server, device, display and scope, local one-session activation, policy/identity validation and revocation; no persistent background listener. X11 input candidate with XTEST, single-monitor and active local unlocked logind checks; unit and disposable Xvfb fixtures, Linux archive workflow. Fixed known macOS Swift duration compile error. Latest CI pending. Wayland portal, additional power operations, virtual display, privacy mode, long-running supervised service and physical acceptance remain open.

### M4 third integrated batch
374e77f four CI succeeded; a6a4065 dual native packages passed. New implementation: Wayland ScreenCast/PipeWire view-only portal (versions 2–5, one monitor/window/virtual source, compositor-provided virtual capability), independent host-reviewed power request, and locally activated bounded three-session/30-minute saved-policy service. CI only verifies portal compile/element/frame conversion and inert power fixtures; real compositor/OS power/long service acceptance pending. No hidden or automatic login access, no Wayland input, no physical-display privacy, no Windows/macOS virtual display driver. M4 not yet accepted.


### M4 fourth integrated batch
2bb2dc2: all six CI workflows passed. Candidate adds Portal v6+ stable serial targeting, native Wayland monitor+keyboard/pointer via explicit compositor RemoteDesktop grant, native console/desktop/sleep lifecycle checks and fail-closed suspend guard, continuous saved-policy verification, locally authorized 12 sequential sessions/2h option with 15-minute per-wait bound, and local native view-only sharing pause/fresh-frame resume. Physical privacy and Windows/macOS virtual display driver are not implemented; Wayland real compositor and OS lock/suspend behavior still need physical acceptance. New CI pending. M4 is not formally accepted.

### Alpha preview publication gate
Owner approved v0.1.0-alpha.1 after Linux portal compilation repair and successful same-commit CI across three platforms. Six checks, exact-source package manifest and SHA256SUMS required. Publication does not imply M4 completion or real-device acceptance. Release completed on 2026-11-26: [v0.1.0-alpha.1](https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.1) is a published prerelease at `303163c`, with all six CI checks successful and all three platform archives plus SHA256SUMS.txt uploaded. [Publication record](PREVIEW_RELEASE.md#first-publication-record--2026-11-26).

### M4/M5 发布后的安装与环境诊断批次

新增本机运行环境诊断、固定组件清单 SHA-256 校验、离线 VP8 合成编解码、GStreamer 元素缺失列表、Windows 官方运行库路径候选、Linux LAN 合成诊断组件补包、Linux 打包副本移除调试信息、各平台打包后自检及后续发布时归档内组件核验。代码待本轮 CI；已有 alpha.1 标签和文件保持不变。无正式安装器、物理隐私屏、Windows/macOS 虚拟显示驱动或跨重启无人值守新实现；实机验收仍是 M4 退出条件。


### M5 first installer/update/localization integration
Previous 4ee4563 Core 45747955262, Native media 45747955298, Desktop 45747955274 and Linux 45747955300 passed; Windows/macOS 45747955283 still running at integration time. Added current-user NSIS installer and uninstall fixture, macOS drag-to-Applications DMG and read-only image smoke check, Linux DEB with dpkg-shlibdeps and extraction/runtime check; portable archives retained. Approved future preview publication requires all six package formats, exact revision/tag stamps, component manifest, resolved dependency inventory and checksums. Manual version discovery against fixed official GitHub HTTPS release API, semver channel filtering and exact platform assets, no auto-updater. Eight UI language options (two existing complete dictionaries + six partial 125-message dictionaries with English fallback/coverage). System language matching and official runtime links. New CI pending. OS signing, bundling redistributable runtimes, full translations/native viewer localization, license audit, real device/install acceptance and actual M4 closure remain open. No new release authorized.


### M5 second integrated batch

89ba615: Core [45749483652](https://github.com/xzcode/remote-ex/actions/runs/45749483652), Desktop [45749483703](https://github.com/xzcode/remote-ex/actions/runs/45749483703), Linux [45749483693](https://github.com/xzcode/remote-ex/actions/runs/45749483693), and Windows/macOS [45749483630](https://github.com/xzcode/remote-ex/actions/runs/45749483630) succeeded; Native media 45749483656 was canceled, not counted as passed. All six installer/portable formats exist in the successful native artifacts. 4ee4563 all five applicable workflows passed. New batch adds shared startup/offline server configuration validation, no-secret preflight summary, SIGTERM shutdown, non-root signaling-only container and opt-in loopback-default Compose template; isolated container CI checks rejection gates, actual loopback health and graceful stop. Saved server URL/ID profiles in local SQLite with explicit save/remove, no auto-connect or permission grants; apply configuration on presence/routed pages. Updates display validated official SHA-256 metadata and exact tag page links. Six additional partial locales expand from 125 to 221 translated messages each (English fallback remains, native viewer not fully localized). New CI pending. No external server deployed, no credentials created, no new release authorized. M4 physical-device acceptance, native virtual display/privacy drivers, signed updates, full translations and license audit still open.


### M5 third integration batch

Previous e98ef583 Core [45763059710](https://github.com/xzcode/remote-ex/actions/runs/45763059710) and signaling container [45763059725](https://github.com/xzcode/remote-ex/actions/runs/45763059725) passed, including container health/SIGTERM; native desktop/media workflows were still running at integration. New candidate: explicit WSS challenge-only preflight (TLS + pinned ID, no device identity/registration), tests with actual TLS and no registration, optional certificate-only private CA on routed page; official metadata-bound local release archive SHA-256/size/name checker (no extraction/execution); Linux native file picker via XDG portal; responsive native viewer toolbar with shared geometry/hit test, local language propagation, localized ASCII Latin labels and Unicode OS title/legend for CJK (not complete CJK glyph rendering); six partial locale dictionaries expand to 287 entries each. No new release, deployment, unattended access or signed auto-update. New CI pending; actual native viewer/portal file chooser and physical devices need acceptance.


### M5 fourth integrated batch

98cd04a: Core [45763674546](https://github.com/xzcode/remote-ex/actions/runs/45763674546), Desktop [45763674576](https://github.com/xzcode/remote-ex/actions/runs/45763674576), Native media [45763674522](https://github.com/xzcode/remote-ex/actions/runs/45763674522), Linux [45763674521](https://github.com/xzcode/remote-ex/actions/runs/45763674521) and signaling container [45763674555](https://github.com/xzcode/remote-ex/actions/runs/45763674555) passed; Windows/macOS development build 45763674564 was in progress at integration. New candidate completes 423 UI dictionary keys in all eight languages, adds dictionary completeness CI, local system-font rasterization of fixed routed-viewer toolbar labels (CJK/Latin), cached tiles generated off event loop with legible bitmap fallback, Linux Noto CJK fixture; unifies public/private CA selection across preflight, registration, CLI, and media (certificate-only private roots replace public roots), adds private CA to presence UI; enriches build dependency declaration evidence and review flags, and requires container preflight in a future explicitly authorized release gate. No embedded fonts or runtime redistribution, no signed updater, no final project license, no new release or deployed service. New CI, linguistic review, physical install and M4 real-device gates pending. See [third-party review](THIRD_PARTY_REVIEW.md).


### M5 alpha.2 release acceptance candidate

3d98ba8 Core 45765313210, media 45765313251, container 45765313211 and Linux 45765313158 succeeded (including actual installed CJK font fixture, Xvfb and DEB). Desktop 45765313261 failed before compilation because the default working directory was apps/desktop and scripts/ci/check_locales.py was looked up there. This candidate fixes the step working directory and script root resolution, and requests v0.1.0-alpha.2 as an experimental prerelease. Seven exact-commit CI workflows required; each native build stamps its two package hashes and publisher cross-checks all six. RELEASE_BUILD.json and SHA256SUMS.txt record run IDs and verified package hashes. New candidate CI and actual publication pending. See [M5 acceptance](M5_ACCEPTANCE.md), [M6 plan](M6_PLAN.md). No physical acceptance, official publisher signature, final license or public service deployment claimed.


## alpha.2 publication verified — 2026-11-26

[v0.1.0-alpha.2](https://github.com/xzcode/remote-ex/releases/tag/v0.1.0-alpha.2) was published at 06:32:30 UTC (14:32:30 Malaysia time), a published prerelease, not a draft. The Git tag and release target both point to `6a42d6e1fb85d6ecee90a2486e01ac165ead14cf`. All seven exact-commit runs succeeded: [Core](https://github.com/xzcode/remote-ex/actions/runs/45766782091), [Desktop](https://github.com/xzcode/remote-ex/actions/runs/45766782111), [Native input](https://github.com/xzcode/remote-ex/actions/runs/45766782214), [Native media](https://github.com/xzcode/remote-ex/actions/runs/45766782370), [Windows/macOS](https://github.com/xzcode/remote-ex/actions/runs/45766782137), [Linux](https://github.com/xzcode/remote-ex/actions/runs/45766782074), [Container](https://github.com/xzcode/remote-ex/actions/runs/45766782115). [Publisher](https://github.com/xzcode/remote-ex/actions/runs/45768863134) passed every step. GitHub API confirms eight uploaded assets (Windows Setup 7,837,600 bytes and ZIP 16,225,272; macOS DMG 18,262,344 and TAR.GZ 16,769,943; Linux DEB 25,822,306 and TAR.GZ 37,927,547; SHA256SUMS.txt and RELEASE_BUILD.json). The physical acceptance, OS publisher signing and third-party license review gates remain open. No public deployment occurred. No existing tag or asset has been changed.

### M6 first integrated candidate

Shared 28-profile v1 table now drives routed grants and signature validation without changing code values or legacy profile-0 verifier semantics. Offline --protocol-info, public deterministic Rust/WebCrypto fixture vectors, local staged allowlist creation/enrollment/revocation/diff, shared startup parser, and bounded operator-configured TURN pool with region/priority/route-stable choice. All new checks pending; no DNS health probes, active config changes, auto restart, live management endpoint, browser/mobile real session or new release. See [M6 first handoff](M6_FIRST_HANDOFF.md).


### M6 second candidate — browser compatibility laboratory

02031a8 completed all six applicable workflows successfully: [Core](https://github.com/xzcode/remote-ex/actions/runs/45770433146), [Desktop](https://github.com/xzcode/remote-ex/actions/runs/45770433415), [Native input](https://github.com/xzcode/remote-ex/actions/runs/45770433231), [Native media](https://github.com/xzcode/remote-ex/actions/runs/45770433201), [Linux](https://github.com/xzcode/remote-ex/actions/runs/45770433259), [Container](https://github.com/xzcode/remote-ex/actions/runs/45770433169). The shared contract, offline enrollment staging and relay selection fixtures passed. The new isolated apps/web lab uses actual Snow 0.10.0 in Rust WASM, browser WebCrypto non-extractable ephemeral Ed25519, fixed profile-0 mutual route signatures and replay rejection, native Rust responder ↔ Chromium WASM initiator fixture, same-page WebRTC generated canvas video only after exact signed SDP and connected transport certificate fingerprints match, decoder and actual pixel verification, explicit 30-second consent, stop/visibility cleanup. A loopback-only static server and real Chromium workflow build/test/package the lab; new results pending. Not external WSS signaling, true desktop viewing/control, persistent key storage, Web/mobile platform acceptance or a new release. See M6_WEB_LAB_HANDOFF.md.


### M6 third candidate — actual WSS synthetic media fixture

1c48ad8 Core [45773246555](https://github.com/xzcode/remote-ex/actions/runs/45773246555) passed. Web [45773246513](https://github.com/xzcode/remote-ex/actions/runs/45773246513) native Noise/replay and WASM compile succeeded, but Playwright 1.56.1 could not install Chromium dependencies on the ubuntu-latest 26.04 runner; no browser result. This candidate pins the browser CI to Ubuntu 24.04, adds exact administrator-configured Web Origin policy (TLS and device allowlist required; native no-Origin connections retain prior behavior), synthetic-profile-only bounded encrypted Offer/Answer/Proof with actual Rust signature verifier and strict order, and a disposable real rendezvous WSS/browser fixture with independently installed test CA, wrong-origin and wrong-server-ID rejection, generated registered keys, actual route and mutual Noise/SDP proofs, DTLS certificate checks and decoded synthetic video. WSS test script not shipped in user bundle. No real Web desktop viewing/control, remote deployment, new release or mobile implementation. New CI pending. See M6_WSS_HANDOFF.md.


### M6 fourth candidate — persistent browser identity and native record order

996a054 Core, Native media, Container and Web lab passed; [Web lab](https://github.com/xzcode/remote-ex/actions/runs/45774460916) includes actual WSS synthetic decode. Candidate adds explicit origin-local non-extractable browser identity vault, reload/isolation/deletion checks, and profile-0 native-order encrypted compat quality and host ICE batches around exact signed SDP. New CI pending. No persistent identity registration, native GStreamer/browser media interoperability, real desktop view, input, mobile, public deployment or new release. See [handoff](M6_NATIVE_ORDER_HANDOFF.md).


### M6 fifth candidate — independent native VP8 ↔ browser fixture

c757557 Web CI Rust tests passed; full browser result pending when work started. New CI-only enrolled localhost WSS fixture connects the persisted browser key (after reload) to the independent no-feature Rust/GStreamer synthetic host, uses native profile 0 quality/SDP/encrypted ICE, negotiated data channel, DTLS certificate verification and requires 30 decoded 960x540 frames, pixel evidence, fixed inert input release, and route closure. Handles native ICE end-of-candidates and bounded config/offer races. No actual desktop capture, user input, public connection or new release; interoperability remains pending CI. See [handoff](M6_NATIVE_MEDIA_HANDOFF.md).


### M6 sixth candidate — explicit browser read-only controller and actual X11 fixture

c757557 Web CI passed in full; 25c8be4 Rust/WASM and two-browser WSS passed at integration time; native build pending. Corrected missing supervised flag for independent synthetic host fixture. New source-only opt-in view UI, controller-only profile-1 WASM, full key/pin and one-shot approval, compat VP8, signed encrypted ICE and view metadata, host readiness/DTLS gate, read-only layout switching, bounded heartbeat/stop/closure; native temporary supervised view enrollment barrier. CI adds actual authorized Xvfb capture test. Direct ICE only, no real input or external deployment, not in alpha.2. See [handoff](M6_WEB_VIEW_HANDOFF.md).


### M6 seventh candidate — approved ICE policy and explicit Web input

48733f2 Core passed; full native browser integration still pending at start. Added operator allowlisted STUN/TURN, short-lived credential checks, native URI conversion, signed forced-relay policy and selected pair verification; separate explicit profile-2 input candidate gated by operator option, exact host grant and locally focused activation after decoded media. Bounded protobuf, physical keys, geometry/letterbox, wheel and release handling; pure Node tests. Direct read-only remains default, no unattended or automatic reconnect. Real control and cross-network relay acceptance pending. See [handoff](M6_WEB_CONTROL_NETWORK_HANDOFF.md).


### M6 eighth candidate — mobile touch controller

48733f2 Core, Native Media and Linux X11 CI passed. New explicit touch modes, single pointer lifecycle, compatibility page and narrow-screen UI. Pure touch test and CI added. Physical phone/browser acceptance, native mobile packaging and original browser-native integration still pending. See [mobile handoff](M6_MOBILE_HANDOFF.md).


### M6 Android packaging candidate

Shared mobile asset staging, explicit operator endpoint/ICE policy, offline default Android private WebView shell and debug APK CI. No background session, native mobile host, Android hardware-backed keys, native iOS app or physical acceptance. See [mobile build](../apps/mobile/README.md).


### M6 ninth candidate — iOS shell, multi-origin enrollment and Android SDK repair

f41c864 complete Web CI passed actual independent native VP8 and authorized Xvfb capture. Android 8b79c8b WASM/staging passed; SDK command missing before APK compile. Candidate provisions SDK tools, adds offline-default iOS 17+ WKWebView project and unsigned simulator build, plus mutually exclusive bounded exact multi-Origin configuration, preflight count and browser tests. Actual mobile compatibility/control, full input/relay acceptance, production deployment and signing remain open.


### M6 tenth candidate — extended foreground sessions and runtime quality

Existing profiles 22/23, bilateral 420s route lease and 300s local consent; operator/user explicit opt-in, no silent fallback or renewal. Native quality D2/D3, bounded actual decoder dimensions and full-resolution-only input activation/release, mobile asset staging. Added pure policy and actual Xvfb >30s/quality-switch CI candidate. d8a052a Core/container and iOS simulator job passed, Android and latest browser integration pending. Physical devices, real input/TURN, operations and signing remain open. See [handoff](M6_EXTENDED_HANDOFF.md).


### M6 eleventh candidate — actual browser control protocol acceptance

6c84eeb Core and both offline mobile jobs passed. New separate feature-gated, loopback-only inert host shares native profile 2 auth/media and Dispatcher; Chromium uses real controller UI, verifies saved details require fresh consent, explicit activation, geometry, key/button/wheel, host-confirmed ReleaseAll while held, and route close. No OS injection or desktop permission bypass. Latest integration CI pending. See [control acceptance](M6_CONTROL_ACCEPTANCE.md).

### M6 operational acceptance checkpoint

As of 74af07c, full Web regression passed at a72bbcc ([45802920103](https://github.com/xzcode/remote-ex/actions/runs/45802920103)); actual browser control against an independent inert Rust dispatcher, trusted touch, Xvfb view, forced TURN and extended quality tests are recorded. Windows/macOS and Linux development builds passed at 8a1263f. Android API 36 legacy WebView 133 correctly refused missing Ed25519, Android modern system image installed but timed out before ADB readiness; 74af07c retry ongoing. iOS simulator install/launch passed at a72bbcc. WebKit at a72bbcc verified actual staged HTTPS identity lifecycle and native read-only connection/decoder frames, but visible pixel verification failed; 4acf53e rendered pixel retry ongoing. No physical mobile/OS input or cross-network device acceptance. See [mobile runtime evidence](M6_MOBILE_RUNTIME_ACCEPTANCE.md). HTTPS live deployment verifier and signaling container preflight passed at ef9bc4b. This new offline operator tool ties an independently approved static tree to actual rendezvous configuration and contract without starting services; new dedicated deployment CI pending. No public deployment or release.

### M7 UI/UX redesign — first workspace batch

User approved TeamViewer-inspired workflow (original Remote EX visuals). New desktop home/quick invitation review, host preparation, separate server-scoped device/history workspaces, file transfer route, unified controls/outline icons and soft gray theme; eight-language labels. All native grants/transport behavior preserved. See [M7 design](M7_UI_REDESIGN.md). Mobile redesign follows in the same milestone; M6 physical/Android/WebKit media gates remain separate. New build and UI evidence pending.

### M7 second desktop workspace batch

8b8557b desktop visual, desktop checks, Linux candidate and Web lab passed. New native top actions and bottom actual statistics strip, approved-catalog dual-pane browser, file mode shortcuts and scoped search/sort with preserved immutable indexes. Eight-language desktop labels added. No protocol or authorization changes. See M7_UI_REDESIGN.md. New CI pending; mobile gestures/virtual keyboard/localization and physical acceptance remain open.

### M7 PC control-window visual polish

User requested less plain PC session UI. New native chrome uses actual shared geometry, compact rounded icon controls, system light/dark, grouped live-only metrics and generated preview artifacts in Linux X11 CI. 1881285 Windows/macOS development builds and native tests passed; 99bdbc5 Web integrated UI passed. Latest native refinements require build/render verification. Mobile virtual keyboard and advanced gestures remain next; no release or physical acceptance.

### M7 mobile input usability

6caaa92 PC chrome/core/native/Linux/Windows/macOS/Web CI passed; actual renderer preview artifact in run 45926987757. Added smart single-finger pointing/tap/long-press and foreground virtual HID keyboard with local one-shot modifiers. Preserved explicit authorization and separate input ownership. New integration pending; advanced multi-touch zoom/scroll, IME and physical mobile runtime remain separate. No release.

M7 follow-up: 857ab1b Web integrated keyboard/dispatcher and actual isolated X11 direct/TURN, native media, Linux and core passed. Explicit two-finger scrolling added with separate input owner; latest physical/mobile/WebKit gaps remain open. See M7_UI_REDESIGN.md.
