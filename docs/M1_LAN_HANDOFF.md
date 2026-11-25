# M1 第五批交接 — LAN 信令与可靠数据通道

2026-11-25

## 已验证
- Windows/macOS Desktop checks #45569412142、Core CI #45570425414 成功。
- Native media engine #45570342512 成功：普通 VP8 WebRTC 与原生 RGBA appsrc 双端同进程回环各至少 30 个真实解码帧；此前 ICE 结束标记导致的失败已修复。
- Core CI #45570749829 成功：新增有界 LAN 信令消息和 loopback TCP framing 测试。

## 新增代码
- 5b34843：crates/signaling，版本化有界 JSON/TCP framing、明确本地地址、一次连接、会话隔离、读写与总超时。仅诊断，非 TLS 生产信令。
- c21fda1：GStreamer 1.22+ 可靠有序数据通道、队列/消息大小/发送积压限制；lan_diagnostic CLI；双方独立进程临时 Ed25519 公钥、明确本地许可、一次性双签名、ICE、实际 WebRTC/DTLS/SCTP、固定模拟输入 → SessionGate → 回执。支持明确的私有 LAN 地址；CI Python harness 仅使用 loopback，不启动长期服务。真实屏幕与 OS 输入均不在此路径，不能宣称可以控制电脑。
- Native media engine [#45570886138](https://github.com/xzcode/remote-ex/actions/runs/45570886138) 与 Core CI [#45570886109](https://github.com/xzcode/remote-ex/actions/runs/45570886109) 已触发，最终结果待查。CI 将运行两种历史 loopback 和双独立进程诊断。
- 未改动用户服务器、未部署服务、未发行。依赖锁、GStreamer 跨平台运行时、真实窗口/屏幕权限实机验证及第三方许可审查仍待完成。

## 下一步
1. 检查最新 CI，修复独立进程协商/数据通道失败；保存实际运行日志。完成两台真实设备的合成视频与测试数据路径，验证独立网络接口。
2. 将诊断 CLI 的连接流程抽象成会话 owner 并接入 Tauri UI，建立真实会话提示、断开/撤销、独立密钥存储、明确可信设备校验。生产需要保护信令通信、验证实际 DTLS 对端与签名 SDP 的绑定，不将测试协议直接暴露公网。
3. 接入经本地授权的连续屏幕帧与原生渲染窗口，修正原型缩放和 stride；仅认证授权后开放真实媒体。
4. 实现正常系统权限下的键鼠平台适配，串行化输入/授权撤销/断开清理；实际测试须在授权设备上进行。
5. M1 退出门槛仍是真实两台电脑之间可见、受控、可断开的远程桌面闭环。M0 媒体跨平台及分发验证仍未完成。
