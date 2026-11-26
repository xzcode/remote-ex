# M6 第二批交接：本机 Web 加密及合成媒体实验

基线 02031a8 六项 CI 全通过。alpha.2 仍指向 6a42d6e，已发布资产不变。

## 已实现候选
| 项目 | 实现 |
|---|---|
| WASM Noise | 独立 Rust crate，Snow 0.10.0、现有 RouteBinding v1、仅固定 synthetic profile 0，双向签名验证后才允许固定测试记录；失败关闭状态。 |
| 浏览器密码学 | WebCrypto 公共向量、临时不可导出 Ed25519 私钥、SHA-256、实际 Noise transcript 签名、修改证明/重放拒绝。 |
| 原生互通 | CI test-only Rust responder 经固定有界 stdio 与 Chromium WASM initiator 真正交换握手、签名及双向固定密文，不开启端口。 |
| 合成媒体 | 同页两个 RTCPeerConnection，精确签名 SDP，读取实际 connected DTLS transport certificate stats 并核对双方指纹，随后附加 320×180 canvas 生成视频，检查真实 decoder 帧及像素。 |
| 用户界面 | 明确勾选授权、开始/断开、六个验证阶段、真实解码计数/分辨率、跟随系统浅深主题、自适应布局。30 秒限制，隐藏/离开页面停止；失败清理。 |
| CI / 分发 | 独立 Web synthetic compatibility lab workflow：native test、wasm32、Chromium、合成媒体及清理；通过后生成临时本机实验档案，非 Release。 |

## 安全与边界
仅可在 localhost 启动固定静态服务器；CSP、Permissions-Policy 禁用摄像头、麦克风、屏幕捕获及剪贴板等；没有真实设备地址、注册、网络信令、桌面捕获或输入功能。浏览器自行生成本机 host ICE 候选，只在同页两端交换，不配置外部 STUN/TURN。两个本机角色由同一实验页面管理，不构成真实两端授权或独立信任。真实 v1 SDP/ICE 必须经同一已验证 Noise route 传输；本实验独立验证 Noise 和真实本地 WebRTC，不宣称外部 v1 路由建立。原生测试进程不进入浏览器档案。

## 待验收
新 Web CI 尚待执行结果。需要实际编译 WASM、验证原生↔浏览器握手及 Chromium DTLS 证书字段/帧。其他浏览器、WebKit/Safari、Android/iOS、持久密钥、真实桌面互通与跨网络 P2P/TURN 均未完成。M6.4/5 为候选而非阶段关闭；M6.6–10 尚未开始。本轮不自动发布/部署任何 Web 服务。
