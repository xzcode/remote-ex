# M1 第三批交接 — 原生媒体引擎

2026-11-25

## 已确认
- Core CI #45564676252 成功（早期 Rust 核心）。
- Synthetic media probe #45569412163 成功：本地合成视频 VP8 编码→解码及 WebRTC 插件可用。
- Desktop checks #45569412142：macOS 全部成功，Windows 前端及 Rust 编译成功、最后测试运行中（记录时）。
- 不以编译成功代替实际桌面采集和权限验证。

## 代码
- 3dbac138：crates/media 独立 Rust/GStreamer workspace。VideoConfig、Peer::sender/receiver、VP8 RTP、真实 webrtcbin SDP offer/answer、ICE 交换、连接状态、GStreamer bus 错误、有限队列、native appsrc RGBA 输入、appsink GstSample 输出与 RAII pipeline shutdown。loopback CLI 连接两个实际 WebRTC peer，至少 30 解码帧及 Connected；使用合成画面，非屏幕共享。
- 89e67e97：session.rs 会话 transcript、签名与验证、预期公钥及原始 SDP 哈希检查；篡改角色/权限/nonce/SDP/公钥的针对性测试。
- 无 UI 连续采集、native renderer、真实设备间信令、远程输入或公网连接。媒体 module 和安全 module 暂未集成。无外部信令监听和部署，真实屏幕未进入媒体发送端。
- Native media engine #45569853979 运行中；最新 Core CI #45569930118 待结果。未宣称新增代码编译或双端 WebRTC 运行通过。依赖锁定/许可证审查仍待完成。

## 后续
1. 核对 https://github.com/xzcode/remote-ex/actions 的新 run 日志，修复 native engine build/loopback 或 core session test 的真实问题。确认 Windows 最后测试状态。必要时保存依赖锁文件。
2. 采用受平台授权的连续采集源（XCap video recorder 能力待验，或系统原生采集）接入 appsrc；限制速率、尺寸、生命周期，避免普通 UI IPC 连续传帧。完成独立原生窗口渲染。
3. 独立配对确认和信任记录；定义本地有界信令、nonce 生命周期及双方 SDP 证明交换，真实 DTLS 连接后才开放经授权的画面。再建立可靠有序 data channel 接入 SessionGate。
4. 真实双设备输入控制、坐标/DPI 映射、权限拒绝/撤销及释放键鼠验证。M0 技术门槛及 M1 退出条件不变。

## 运行
见 crates/media/README.md，Linux runner 安装 GStreamer dev/runtime 依赖后 cargo test/run 独立 workspace。Windows/macOS 原生媒体包和渲染仍待验证。没有发行包。此批未使用或修改任何用户服务器。
