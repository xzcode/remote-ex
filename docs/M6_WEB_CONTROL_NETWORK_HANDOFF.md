# M6 第七批：Web ICE 策略与显式输入候选

基线 f41c864。c757557 完整 Web CI 已通过；48733f2 Core CI 成功；25c8be4 原生媒体独立测试仍未结束，真实 X11 Web 验证待。不得把构建候选视为实际远程控制完成。

- 新增 operator ICE 策略：显式批准一个 STUN 与最多四个 TURN 地址，精确 URI、端口、短期凭据生命周期及字段验证，服务端不可注入任意新地址。将 native URI 转换为标准 RTCIceServer，不持久化临时密码。direct 默认不变；all/relay-only 有界候选、独立签名 relay_only，强制中继验证双方实际 selected relay candidate。无默认公共 STUN/TURN。
- 可选 --allow-control 才显示 Web 控制申请；默认只读。明确选择 profile 2/grant=3，独立主机公钥、Noise transcript 和 SDP 签名，不从只读自动升级。原生 host 仍需完整本机许可与 OS 权限；不会绕过任何授权。
- 已实现纯 v1 Protobuf 编码、序号、物理键码、授权画面内坐标/letterbox、滚轮范围、三键鼠标及 release-all。只有双方已认证、收到 host readiness 且解码帧后才可点击显式启用输入；焦点离开/窗口失焦/鼠标拖出边界/异常时释放并禁用；断线由 host dispatcher 清理。没有全局键盘捕获、剪贴板、OS 输入权限绕过、隐蔽控制或自动重连。移动触控尚未实现。
- 增加纯 Node 协议/ICE 测试与 CI；真实 profile2 远端验收、强制 TURN 跨网测试仍待，禁止声称通过。
- 本批源码模式：node apps/web/serve.mjs 8080 --view wss://your-server/v1/ws [--stun stun://host:port] [--turn turn://host:port] [--relay-only] [--allow-control]。强制 relay 需要 host 同样明确 --relay-only；host 画质 compat。控制端仍需要独立 host 的 profile 2 授权。普通 lab 无连接入口，现有 alpha.2 不变。
