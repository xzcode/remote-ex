# M6 第六批：实际只读 Web 主控候选

基线 25c8be4。第四批 c757557 Web CI [45775548072](https://github.com/xzcode/remote-ex/actions/runs/45775548072) 完整通过。第五批 [45775869679](https://github.com/xzcode/remote-ex/actions/runs/45775869679) Rust、WASM、Chromium 与双浏览器 WSS 通过，独立 GStreamer host 尚在构建。复核发现第五批 native harness 缺 --desktop-session，已有 barrier 要求该参数；本批修复，保持 no-feature synthetic 约束。

## 开发内容
- 源码 checkout 显式启用 local loopback viewer：node apps/web/serve.mjs 8080 --view wss://your-server/v1/ws，访问 /view.html。配置由 operator 启动参数固定；精确 CSP WSS origin；正常 lab 没有 viewer endpoint。仅自建 WSS + TLS 可信链与独立 server ID、设备 allowlist、精确 Web Origin。没有默认公共服务或自动部署。
- Web UI：固定信令地址、完整 server ID 与 host Ed25519 公钥、持久浏览器身份、公钥及确认、连接/审批/鉴权/媒体/停止阶段、解码帧/分辨率/时限、已授权显示器选择。禁止任意 endpoint、未确认自动连接、输入/音频/剪贴板/文件或录屏。
- 新 WASM ViewPeer：仅 controller profile 1、grant=1、native quality compat、原始 SDP 与双方签名、加密 host-only ICE、严格 Geometry 与 ViewLayout 顺序/范围。无 host role、raw encryption 或 fixture data API；验证 profile 0/1 不互认、早期元数据拒绝。
- WebRTC recvonly VP8、externally negotiated data channel id 0，DTLS 指纹及 host READY_ACK/初始布局后才接到 video。定期媒体/信令心跳、帧停滞、页面隐藏/离开/异常即时停止，正常 STOP/ACK/route close。限定约 30 秒 viewing，只有预授权显示器可切换。
- 原生临时只读 host 额外 --enroll-before-register 仅显式监督、非 persistent/armed 的 profile 1；输出 KEY、ENROLL 后等待 REGISTER DEVICE，本机 supervisor 可先完成准入登记；后续完整 peer-key 与 consent 流程不变。未接入 Tauri GUI 自动管理。
- 新真实 X11 capture fixture：CI hosted runner 的独立 Xvfb，实际现有 routed_x11_view 原生进程、WSS 服务、Chromium 页面；验证错 server ID、显式登记、真正 Xvfb 彩色像素、960x540 frames、stop 与 host release。此测试不代表实际用户电脑或跨网络验收。
- 该 Web view 候选暂不进入普通 lab 归档或 alpha release。原有诊断页/测试保留。

## 范围与剩余
仅 direct host candidates + compat 960x540，且 server ICE config 必须为空。浏览器/原生 mDNS 支持、STUN/TURN、跨网、真实物理机、Wayland、macOS/Windows、撤销与重连压力测试仍未完成。新 CI 未通过之前不能称可用；M6.4/5 未正式关闭。M6.6 真输入及 M6.7–10 未实现。禁止用浏览器 profile0 模拟测试证明真实授权。
