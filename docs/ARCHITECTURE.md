# 技术架构与决策 ADR v0.1

## 决策
- ADR-001 已确认：自主开发，复用成熟底层库；不 fork RustDesk。
- ADR-002 工作基线：Tauri 2 + Vue 3 + TypeScript，Rust 核心/Tokio；具体版本在首次构建时锁定。
- ADR-003 工作基线：WebRTC + ICE/STUN/TURN；信令 Axum + WSS；coturn；SQLite；消息类型以 Protobuf 为目标。最终媒体库及分发方式在 M0 spike 后冻结。
- ADR-004 工作基线：局域网采用本地受控的信令端点与直接身份确认，不依赖公网服务；公网可选自建注册/信令、中继。WebRTC 自身不提供业务身份验证。
- ADR-005 待决：项目许可证、最低支持 OS、媒体引擎/原生渲染与硬件编码分发方案。MIT 是候选而非已授予许可。

## 逻辑架构

```mermaid
flowchart LR
  A[主控桌面 UI] --> AC[主控 Rust 会话核心]
  B[被控桌面 UI] --> BC[被控 Rust 会话核心]
  AC <--> S[可选 Axum 信令]
  BC <--> S
  AC <-->|WebRTC 直连| BC
  AC <-->|加密包| T[coturn 中继]
  T <-->|加密包| BC
  BC --> CAP[平台采集与输入适配]
  AC --> R[原生视频渲染]
```

信令不转发媒体；TURN 不终止媒体端到端加密。注册与授权不是同一件事。双方确认身份和权限后才启动对应资源。LAN 信令不得默认监听所有地址且不得以“内网可信”替代身份校验。

## 模块边界
| 模块 | 职责 |
|---|---|
| apps/desktop | Tauri 生命周期、Vue UI、窗口、托盘、受限 IPC |
| crates/core | 会话状态机、设备管理、能力协商、服务配置 |
| crates/protocol | 版本化 wire schema、消息类型、大小与语义验证 |
| crates/security | 设备密钥、身份确认、授权校验、信任撤销 |
| crates/transport | WebRTC 适配、ICE、数据通道、连接统计 |
| crates/capture | 屏幕枚举、帧、格式、时间戳、游标 |
| crates/codec | 媒体 pipeline 与编码/解码能力选择 |
| crates/renderer | 原生或零/低拷贝渲染、缩放、坐标映射 |
| crates/input | 键鼠事件、按键状态、坐标映射、权限验证 |
| crates/audio | 音频设备及媒体轨 |
| crates/clipboard | 双向授权、格式与大小限制 |
| crates/file-transfer | 独立流控、校验、冲突策略 |
| crates/platform | OS 权限、后台服务、进程 IPC |
| servers/signaling | 注册、挑战认证、会话路由、限流、TURN 临时凭据 |
| deploy | 自建信令与 TURN，环境变量模板、TLS 与部署文档 |

目录按实际实现逐步建立，不创建无功能占位模块来宣称完成。

## 媒体数据路径
原生屏幕采集 -> 像素缓冲/颜色转换 -> 编码 -> RTP/WebRTC -> 解码 -> 原生窗口 GPU 渲染。Tauri IPC 仅用于控制、事件和统计，禁止逐帧 base64/JSON。GStreamer 优先验证 webrtcbin、平台硬件编码、接收端解码与原生窗口渲染；若不能满足可维护性/打包要求，再比较 libwebrtc 或 webrtc-rs + 独立媒体 pipeline。不能假设这些方案的互操作和硬件能力已经验证。

## 平台适配
| OS | 采集候选 | 输入候选 | 编解码候选 | 注意事项 |
|---|---|---|---|---|
| Windows | WGC / DXGI | SendInput | Media Foundation | DPI、UIPI、安全桌面；服务与用户会话隔离 |
| macOS | ScreenCaptureKit | CGEvent | VideoToolbox | TCC 屏幕录制/辅助功能，签名及权限持久性 |
| Linux X11 | X11/XShm | XTest | VA-API 等 | 显示会话与权限 |
| Linux Wayland | PipeWire + ScreenCast Portal | RemoteDesktop Portal/EIS | VA-API 等 | Compositor 差异；用户授权不可绕过 |

## 数据与部署
本地 SQLite：settings、devices、trusted_peers、session_history、transfer_history；私钥/敏感凭据单独由平台安全存储或受 OS 权限保护的密钥存储管理，禁止硬编码全局密钥。服务器 SQLite 初期仅保留 device_id/public_key/注册状态/必要审计；在线与路由状态短期内存保存。多实例扩展再考虑 PostgreSQL/共享状态。部署可独立替换信令与 coturn，默认无公共实例地址。信令 HTTPS/WSS，TURN UDP/TCP/TLS；按部署环境显式开放防火墙与中继端口，关闭开放匿名 relay。

## 上游参考
- https://v2.tauri.app/concept/architecture/
- https://gstreamer.freedesktop.org/documentation/webrtc/
- https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html
- https://github.com/coturn/coturn
- https://developer.apple.com/documentation/screencapturekit
- https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api
- https://flatpak.github.io/xdg-desktop-portal/docs/

## ADR-006：M1 单帧本地诊断实现

Tauri 2/Vue3 桌面应用采用独立 Cargo workspace，避免原生 GUI 依赖污染核心 CI。XCap 0.9.10 是显示器枚举和单次本地 PNG 预览的候选依赖；只在本机明确操作后执行，限制尺寸、并发及频率，不持久化图片、不开放网络服务。Tauri raw IPC 只用于有限的单次诊断，连续媒体不得使用该接口。XCap 并非已冻结的视频引擎；连续屏幕采集、GStreamer/WebRTC 媒体及低拷贝渲染仍需实际验证。平台编译与依赖许可证尚待确认。

## ADR-007：原生 WebRTC 媒体模块

crates/media 独立 Cargo workspace，GStreamer 1.20+ 与 gstreamer-rs 0.23。初始 VP8/RTP，webrtcbin 信令动作、SDP/ICE、有界事件队列，appsrc 接收原生 RGBA、appsink 输出 GstSample。两端进程内信令的合成视频闭环是首个运行门槛，不代表公网或实际桌面可用。M1 单视频 m-line，无 renegotiation、数据通道、TURN 或远程监听。媒体和桌面暂未集成。原生帧接入必须由已验证授权的会话 owner 控制；实际身份确认及双方签名校验前只允许合成媒体或不含敏感像素的占位源，真实采集不得提前进入发送管线。

## ADR-008：ICE 完成消息与本地原生画面验证

GStreamer 1.24+ 的候选完成标记可能是空字符串，不能作为无效候选拒绝。保持单 m-line、消息长度及字符限制。M1 诊断增加 appsrc 原生合成 RGBA 输入与可选 autovideosink 渲染，真实屏幕路径由可选 local-screen feature、交互确认和已连接的两个本进程 peer 约束；仍非生产连续采集方案。禁止对外部设备开放真实屏幕或输入，直至完成实际身份/授权和安全信令。后续替换重复截图为原生连续帧源并优化缩放、宽高比、stride、显示器切换及 GPU 路径。

## ADR-009：独立进程 LAN 诊断

crates/signaling 是 portable workspace 成员，实现版本化、有界 JSON 消息及 TCP 长度帧；只用于单连接诊断，不是生产信令服务。显式绑定指定 loopback/RFC1918 地址，限制候选、消息、总时限；无通配地址、公网监听、注册、TURN 或 TLS。JSON 为该诊断控制面选择；原有 Protobuf 输入数据协议不变。生产信令协议需在 M2 冻结，不能默认将诊断格式直接对公网暴露。

GStreamer 原生模块增加协商型（negotiated）可靠有序 SCTP 数据通道；固定 channel id 0、输入 v1、单通道消息限长/有界队列。为使用 send_data_full 提升该媒体模块运行时最低版本至 GStreamer 1.22；原 VP8 loopback 继续验证。

lan_diagnostic 两端各自生成临时密钥，双方在可信途径确认完整公钥并手动批准测试；显式监听/连接、一次性配对、原始 SDP 双签名、ICE 交换、DTLS/SCTP 连接。仅合成视频与固定测试输入序列，验证 SessionGate 的 action 和释放计划，不调用任何系统输入接口。控制端为生成 answer 必须预先应用 offer；此时不交换实际桌面和敏感数据。生产阶段须进一步完成安全评审、持久信任、身份与连接生命周期绑定、加密信令及真实同意流程。

## ADR-010：桌面诊断会话进程隔离

桌面采用固定名称、与自身可执行文件同目录的可信本地构建 lan_diagnostic（Windows .exe）作为诊断媒体子进程；不从 PATH 查找，不提供用户任意程序路径或 shell。Tauri main window 仅暴露 prepare/status/confirm/disconnect 四个受限命令，后端校验具体私网/loopback 地址、公钥长度、显式确认、单活动会话。子进程输出解析固定里程碑，输出/命令有界；监督线程负责180秒截止、取消、退出和 kill/wait，主窗口关闭/应用退出清理。此设计隔离媒体原生依赖，但不是已签名或打包的 sidecar；缺失文件会明确提示。UI 支持双端公钥核对、本地授权、真实连接状态、主动断开、成功/失败与可选合成视频原生窗口，连接进行中禁止切换导航。正式版本需要经过签名的媒体组件打包、进程 IPC 审查、持久密钥、加密信令及真实权限集成。

## ADR-011：独立加密只读屏幕路径

生产候选安全通道采用 Snow 的 Noise KK (X25519/ChaChaPoly/BLAKE2s) 双方预先核对完整临时公钥，原始诊断 TCP framing 不再承载真实桌面会话。独立 lan_view CLI 在两端交互确认、Noise 信令加密、Ed25519 双签名绑定 SDP、实际视频 DTLS 证书 SHA-256 校验、双端 Ready 后，才允许 appsrc 获得 XCap 真实屏幕像素。单视频 m-line、view-only、固定显示器、有界帧与 10fps/960×540 letterbox、30秒会话期限、加密 heartbeat、无远程系统输入/无人值守/文件/剪贴板。无自动升级诊断权限；--test 与真实屏幕 profile 必须一致。当前 CLI 不替换桌面诊断界面、不使用未签名 GUI sidecar 做实际屏幕共享。依赖雪库、GStreamer、XCap 及跨平台运行时需要审计、锁定和实机验证。正式连续采集、持久身份、撤销 UI、更多能力、生产信令/发布均未实现。

## ADR-012：桌面只读会话与原生输入适配

桌面新增 screen / secure_test / diagnostic 三个固定 profile，绑定各自媒体可执行文件名、参数、公钥长度及明确的授权语句。真实 host 必须选择显示器 ID；预先核对 128 hex 完整公钥 bundle，保留本机 UI 中可见状态及断开入口，仍限 30 秒、VIEW-only。lan_view --desktop-session 使用专用监督管道，批准后父进程持续持有 stdin；EOF/额外字节即退出只读子进程。正常命令行真实模式仍要求交互终端。可信组件开发期手工安装，未完成生产签名或校验；真实模式不通过旧明文诊断链路。

crates/input 加入 portable workspace；Dispatcher 串行校验 SessionGate、消息顺序、授权区域、指针状态和速率，跟踪真实尝试的按下键及按钮，失败/关闭时尽力释放；释放失败需报告和重试，不承诺 kill/崩溃时能自动释放。原生候选为 Enigo 0.6.1（Windows/macOS 可选 native feature），仅普通桌面、主显示器和常用物理 HID 键。独立编译与 inert fake backend 测试，未接网络输入/OS 操作命令。后续正式输入 owner 必须完成完整 DTLS/SCTP 绑定、单独本机输入授权、控制端事件采集、屏幕坐标/黑边/DPI 对齐、撤销及正常释放，不能复用只读授权。

## ADR-013：原生控制原型及开发构建

新增 lan_control 独立 profile，VIEW|INPUT 双方授权，Noise 信令、签名 SDP、真实视频与 SCTP 证书检查、可信 Ready；默认 --test 仅合成画面与 inert backend，真实 --control 必须在可见交互终端启动。Windows/macOS native-control feature：XCap 主屏、Enigo 普通权限输入、minifb 固定 native 窗口、等比 geometry、焦点及黑边判断、物理键映射。输入 dispatch 与资源释放同进程；不使用强制杀进程作为正常控制断开。30秒限制，心跳2秒、Ctrl-C、Escape、窗口关闭、传输失败结束。控制端关闭前 ReleaseAll，停止消息后被控端 Dispatcher::close，再发送释放回执。异常硬退出、阻塞原生采集、OS release 失败需要后续改进。只读 GUI 保持独立；真实控制仍未接到桌面会话管理。

新增 Windows/macOS development-build workflow，官方 GStreamer 1.26.11 SDK + SHA256，构建 desktop 和媒体程序的同目录测试包；macOS ad-hoc 签名，Windows 无签名，GStreamer runtime 用户单独从官方安装。CI artifacts 并非正式 release。运行时打包/插件许可、稳定签名和安装器仍需完成。
