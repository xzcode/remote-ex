# M6 第三批交接：真实 WSS 与加密合成媒体实验

基线：1c48ad8。Core CI 45773246555 通过。Web lab 45773246513 原生 Rust Noise 签名/重放测试、wasm32 构建和绑定生成通过；Playwright 1.56.1 不支持 Ubuntu 26.04，依赖安装失败，浏览器测试未执行。本批固定 Ubuntu 24.04。

## 实现候选
| 模块 | 本批实现 |
|---|---|
| Web Origin | 显式精确 Origin 配置，默认拒绝浏览器 Origin；启动校验 TLS 与非空设备准入名单，拒绝 wildcard、无效或重复 Origin；原生无 Origin 客户端仍按现有认证。 |
| WASM SDP | 仅固定 synthetic profile 0；复用原有 Snow 与 RouteBinding。在相互签名通过后允许 host Offer → controller Answer → host Proof → controller Proof，精确原始 SDP 及 Rust verify_session 检查；拒绝其他消息和记录混用。 |
| WSS fixture | Chromium 两个临时生成的身份，用真实 rendezvous v1 TLS/WSS 注册、显式准入、路由申请/同页合成授权、Noise 握手、加密 SDP 与双签名、空 ICE 配置、实际 DTLS 证书、生成 canvas 画面及解码帧/像素，最后 route close。 |
| TLS fixture | 一次性 CA 和 localhost IP SAN 证书，导入独立临时 HOME/NSS 信任库；不忽略证书错误，不修改实际用户/系统信任库。检查正确 pin、错误 pin、非允许 Origin。 |
| CI | 浏览器工具安装、旧 Web lab 与新增 WSS fixture 分别执行，前者失败也会尝试后者（构建前置成功时）；两者都通过才生成普通本机 lab 档案。 |

## 范围
以上新增代码待本批 CI 验证。两端是同页生成的测试身份，不是真实远端用户的独立批准。WSS 集成脚本不打包至普通 lab，普通页面依旧没有 WSS 连接入口。仅一次性本机合成画面，不支持用户填写远程地址、真实桌面采集或远程输入、不自动部署服务器或发布新版本。浏览器连接现有原生 GStreamer 主机、跨网络 ICE、真实媒体查看、持久密钥与用户撤销需独立实现/验收。M6.4/5 尚未关闭，M6.6–10 未实现。alpha.2 保持不变。
