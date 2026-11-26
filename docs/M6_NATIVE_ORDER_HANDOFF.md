# M6 第四批：浏览器身份存储与原生协议顺序实验

基线 996a054：Core、Native media、Container 及 Web lab 自动化成功；Web lab [45774460916](https://github.com/xzcode/remote-ex/actions/runs/45774460916) 的真实 Chromium WSS fixture 成功。alpha.2 仍指向 6a42d6e，不修改 release。

本批候选：
- 独立本地浏览器身份页，显式创建、显示完整 SHA-256 设备 ID 与 Ed25519 公钥、刷新、明确确认删除；IndexedDB 保存不可导出的 CryptoKey，读取时验证算法、用途、指纹及随机域隔离签名。add 防止跨标签覆盖；存储失败不回退到可导出密钥。清除浏览器数据/更换 origin 会丢失或隔离身份。仅删除本地身份不等于服务器撤销。
- 保留原本 profile 0 / 合成-only。可选原生顺序实验：Noise 双证明→加密 compat Quality 双确认→真实 ICE config 请求（仅空配置）→精确 Offer/Answer→双 SDP 签名→加密 IceBatch 双确认→实际 DTLS 证书校验→生成视频。ICE 只允许有界 host candidate；没有外部 STUN/TURN。SDP 不附带候选，真实浏览器候选经加密批次传递。不提供任意明文加密 API；错序/重放/坏候选失败关闭。
- CI 添加浏览器身份创建、跨 reload、禁止导出、禁止覆盖、独立 context、删除检查；新增 Rust record shape 与状态门测试，旧浏览器、WSS、原生 Rust fixture 保留。
- 本批待同提交 CI；不宣称原生 GStreamer ↔ 浏览器已经互通。浏览器持久身份未接入 WSS 注册；页面不允许远程地址、实际远程屏幕或输入。下一批应使用独立原生 GStreamer synthetic host 验证真实 VP8 / data channel / ICE，再扩展仅 view profile 1、独立授权与真实媒体。M6.4/5 尚未关闭，M6.6–10 仍待。
