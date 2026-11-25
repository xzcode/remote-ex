# 第十二批交接 — M1 跨平台运行修复 / M2.1 本地信令服务

2026-11-25

## M1 已确认
- #45575817861 最新桌面 CI：Windows 与 macOS 完整通过。
- #45576282390：三种双端撤销/正常释放集成测试通过，仅 inert/synthetic。
- #45576059718：两平台 SDK、desktop Release、native-control Release 及原生输入单测通过；实际合成控制运行测试失败，未生成 artifacts。macOS 缺 LC_RPATH；Windows 10035 WouldBlock。
- 85acd0b：官方 framework 固定运行时 rpath，Mac 开发 .app 与 ad-hoc 校验，BUILD_INFO。ed2aca9：接受 TCP 后恢复 blocking、保留 deadline、添加跨平台 socket 回归测试。新开发包 #45577251479 及新媒体 #45577251490、核心 #45577251526 已触发，最终结果待查。#45577182682 为仅 Mac 修复的上一轮开发构建，未含 Windows 修复。

## M2.1 本轮实现
- 843b9a5、fdefb70：servers/signaling，Axum/Tokio WebSocket，本机 loopback-only，挑战注册、完整 fingerprint ID、重复在线拒绝、连接请求、明确 host grant/decline、双方 nonce、会话内 role/SDP/proof/ICE 顺序及额度、过期/断开/关闭清理。消息/连接/队列与速率边界。无真实媒体或输入路由。
- Core CI #45577085766 成功，两个 Hub tests 与两个真实 WebSocket integration tests 均通过。服务器不校验实际双方 DTLS、未持久存储密钥、无 TLS，不能作为公网服务部署。M2 后续阶段仍未完成。

## 下一批
1. 核对 #45577251479 最新双平台开发包。依据失败日志修复，不跳过运行测试；成功才给出实际 artifact 链接，区分构建 SHA 与当前 HEAD。外部 GStreamer runtime，Windows unsigned / macOS ad-hoc，无正式 installer/Release。
2. 按 M1_DEVICE_ACCEPTANCE.md 在授权物理设备验证真实屏幕、键鼠、DPI、撤销和退出；没有授权真实桌面环境时不得标记 M1 完成。
3. 推进 M2.2：持久设备身份、独立可信设备确认与配置、可验证 WSS 传输，客户端注册/心跳与批准状态。M2.3：将既有签名、端到端加密和实际 video/SCTP DTLS 验证接入新路由；先合成集成测试再真实操作。
4. M2.4 STUN/TURN 及短期中继凭据、部署边界与跨网络验收。当前未配置或部署任何公网服务、未改用户服务器、未发布 Release。
