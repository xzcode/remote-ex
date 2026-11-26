# M6 第五批：独立原生媒体 ↔ Chromium 候选

基线 c757557 的 Web CI 45775548072 在接手检查时已通过 native Rust tests，WASM 构建尚在执行。避免取消上一批 CI：Web lab master 运行串行，不替换正在执行的验证。

## 实现
- 新增 CI-only native-wss-fixture.js 及 test-native-wss.mjs；复用真实 rendezvous、Rust no-default-features routed_diagnostic（独立 GStreamer VP8 主机），Chromium 是实际主控进程。
- 浏览器在隔离 context 创建 IndexedDB 非导出密钥，reload 后从存储读取并验证；独立 native host 生成公钥，测试工具预先登记两者、公钥/指纹核对、TLS CA 和精确 Origin、明确 native 端一次性 consent；浏览器使用 controller-only 注册。
- 同一 profile 0 复用 native quality/Noise/SDP proof，处理 ICE config 与密文 offer 竞态，支持 1.24+ 原生 ICE end-of-candidates，同时限制非空候选 host-only。实际 negotiated data channel（id=0、ordered、remote-ex-input-v1）、VP8、DTLS 指纹验证、READY/ACK，固定两条 inert protobuf 消息匹配已有 native fixture、至少 30 帧 960x540 解码与像素检查、STOP/释放确认/明确 route close。
- 仅一次性 localhost WSS、临时 CA/NSS 和 runner；Chromium mDNS 隐藏开关只用于隔离 GitHub hosted runner。没有关闭 TLS 证书校验；无公网部署。没有浏览器键鼠监听，native host 无 native-control/native-x11-view 特性，不接触真实设备。新增 fixture 不进入普通 lab 归档；正常页面没有任意远程地址。
- 提取共同测试证书/启动辅助代码；保持旧 synthetic browser/WSS tests。新 CI 先安装 GStreamer 开发及运行库，构建 no-default-features inert binary，再执行独立互通测试。预览 release 不变。

## 未验证与后续
提交时本批新 CI 尚未完成，不能声称跨运行时实际成功。原生 GStreamer SDP/ICE/cert 兼容性是本批 CI 的实际验证目标。profile 1 真实只读、跨公网 ICE、生产注册界面、可撤销长连接、Web 真实输入和移动端仍待；不提前开放它们。下一步依据本批结果修复互通差异，再扩展只读授权与 native source。
