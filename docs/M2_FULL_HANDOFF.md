# M2 功能整合交接（第十五批）

2026-11-25

## 本轮提交
- cd89d7b：修正 #45579865842 routed_diagnostic 编译失败（重复旧 LAN 函数）。
- 00a4bcb、eaf4902：RouteBinding + Noise NN + Ed25519 签名 handshake hash，独立可信对端，opaque sealed routing；媒体 SDP/proof/ICE 纳入加密协商。
- 4685ff8、4a803e8：session ICE config、coturn 180 秒 HMAC REST 临时凭据、一次/参与方、GStreamer STUN/TURN All/Relay、强制 relay 候选检查、实际 coturn CI harness。
- 1fd737a：显式非回环 TLS + allowlist 部署候选，presence 有限重试，部署与 coturn 模板。无自动部署。
- 14adc4d：Windows/macOS 显式持久身份预准备、SQLite 信任库、密钥冲突拒绝、重连 UI、独立身份核对。
- c7cfb68、97d3a71：共享 routed_session owner，单独 synthetic 与 native view/control，profile/relay 签名绑定，真实 DTLS + 独立授权、主屏、30秒、capture worker 与输入清理、可见桌面会话入口、开发包构建。
- c485e24：实际输入对端 ID 必须与可信 key fingerprint 匹配，已登记身份可选择 synthetic 模式，新增真实重注册回归、systemd 部署示例。

## 实际验证
- M1 已有旧开发包：[#45577251479](https://github.com/xzcode/remote-ex/actions/runs/45577251479)，Windows x64 与 macOS ARM64，构建 SHA ed2aca9（不含 M2）。
- Core CI #45583806655（97d3a71）成功；前序核心与原生输入单元测试已通过，未代表完整 M2 验收。
- 最终代码 c485e24：Core CI [#45584030066](https://github.com/xzcode/remote-ex/actions/runs/45584030066)、Native media engine [#45584030116](https://github.com/xzcode/remote-ex/actions/runs/45584030116)、Desktop checks [#45584030101](https://github.com/xzcode/remote-ex/actions/runs/45584030101) 记录时运行中。包含真实本地 WSS+Noise+DTLS+inert、强制 TURN、重连等验证。开发包 [#45583806921](https://github.com/xzcode/remote-ex/actions/runs/45583806921) 源 97d3a71，不含最后 ID 确认/重连测试；未获得新产物。之前频繁提交取消部分 runs，不宣称通过。
- 未使用真实显示器或执行真实键鼠，未使用用户主机、未部署服务、未创建正式 Release。

## 尚未通过的退出门槛
M2 实现候选覆盖计划的工作包，但不能标为已验收：新媒体/原生/桌面构建及运行测试、真实不同 NAT 两网 P2P/relay fallback、物理屏幕与控制权限/释放、持续在线及实际持久存储、正式服务器运营和防滥用审查待完成。GUI 目前为显式 one-shot 连接，30秒/主屏；在线重连不会恢复系统输入。存在强杀或 OS 失败导致释放未确认的风险。真实用户设备不可用时保持 BLOCKED，不绕过测试。
 
## 下一步
先检查上述 CI 最新结果，基于失败日志修复，不启动新功能或重复大构建。完成强制中继真实运行、核对两平台新开发包与具体 SHA，按 M2_NETWORK_ACCEPTANCE.md 验收。只有全部必要结果和真实网络/设备实测通过，才把 M2 标为完成并进入 M3。部署候选参考 deploy/README.md，实际部署须明确指定主机、域名、TLS、准入和 relay 网络策略。

## 追加验证
- Core CI #45584030066 已成功。macOS desktop check #45584030101 与旧开发包 #45583806921 暴露 trust.rs SQLite commit 错误类型 &str/String 不一致，已修正。以修正后的最新 desktop CI 为准；旧开发包失败，不能宣称存在 M2 安装包。

## 第十六批验收接手

- 已核对日志：#45584030066 核心与 #45584030116 媒体全部成功。WSS direct ICE 30 帧、实际 coturn relay-only 30 帧、inert release ACK；LAN host/controller 监督管道撤销通过，同身份 presence 重注册通过。非物理验收。
- #45583806921 两平台构建失败（旧 SQLite 返回类型）；e2869fc 修正。bb65000 新增隔离双 NAT 自动回退及强制中继测试，且通过 DEV_BUILD 文档更新重新触发原生开发包构建。测试仅使用隔离 Runner 和合成媒体。
- 首先查看 #45585068195、#45585068172、#45584544562、#45585068192 实际结果，失败则按日志修复；不要重复堆功能或反复取消构建。详细验收矩阵见 M2_NETWORK_ACCEPTANCE.md。只有取得真实 M2 artifact 才提供下载链接。两台已授权物理图形桌面及独立网络是剩余验收的必要环境；未提供时保持 BLOCKED。无服务器修改或正式 release。
