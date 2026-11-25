# 第十四批交接 — M2.3 设备 ID 路由与媒体集成

2026-11-25

## 已证实
- M2.2 Windows/macOS Desktop checks #45578147789 全部通过；核心 #45578147785 通过。
- M1 Windows 开发包 [#45577251479 artifact](https://github.com/xzcode/remote-ex/actions/runs/45577251479/artifacts/15005652383) 已生成：4,413,956 bytes，构建源 ed2aca98bbdfeee490e1ae2001e146cf5c4947ad，2026-12-09 到期。登录 GitHub 下载。Windows SDK、完整 release 编译、Noise/TCP、inert input release 三场景与上传通过。依赖官方 GStreamer 1.26.11 MSVC x64 runtime；unsigned，非正式 installer/Release。该产物不包含 M2 新代码。
- 同 run macOS 已完成编译及 GStreamer 动态库链接检查，最终测试待查。#45577182682 的旧版本已确认 rpath 修复生效，但未含 socket 修复，运行失败。
- #45579568307 核心 CI、#45579568305 Native input checks 通过（46975e1）。

## 本轮实现
- 46975e1：OutgoingPairing 持有本机一次性 nonce/预期对端/原始期限，服务器返回 ID 后消费绑定。IceBatch 消息边界与服务端双方 Proof 后的一次性转发规则；单元/实际 WS 路由测试更新。
- 10ef04e：routed_diagnostic，独立 WSS 设备注册与路由、完整公钥核对、明确固定测试授权、双方 nonce、签名原始 SDP、实际 video/SCTP DTLS 证书、直接 data channel Ready/ACK、synthetic native RGBA→WebRTC、固定模拟输入→SessionGate/Dispatcher→释放确认，显式路由关闭。只支持合成视频/inert backend；无真实画面或系统输入模式。WSS 信令仍可被服务端读取，不是端到端信令保密。
- scripts/test_routed_diagnostic.py 使用临时 localhost 测试证书、实际 rendezvous TLS server 和两个独立媒体进程，检查完整链路。新增 CI #45579708756 待结果；Core CI #45579708473 同样待查。未部署服务、未改用户服务器。

## 接手重点
1. 检查 #45579708756 新路由集成、#45579708473 core 和 #45577251479 macOS 最终包；针对实际失败修复。可下载产物必须对应成功的具体 SHA。
2. M2.3 生产候选：端到端协商保护、持久可信设备记录与密钥变更、明确当前连接请求授权；在新的路由 owner 内完成真正 GUI/媒体生命周期，不能把 synthetic 诊断授权升级成真实远控。先实现完整合成回环，再处理真实媒体/输入。
3. M2.4 可配置 STUN/TURN、短时授权凭据及受控中继，完成真实 ICE 配置与跨网络验证；服务端目前 IPv4 loopback-only。
4. M1 物理设备验收仍待执行；需要授权 Windows/macOS 真实桌面，继续保留主屏、30秒和私网限制。没有正式 Release 或公网服务。

## 构建结果补记

#45577251479 已确认两个平台 job 全部成功，macOS ARM64 [artifact 15004599699](https://github.com/xzcode/remote-ex/actions/runs/45577251479/artifacts/15004599699)，4,563,606 bytes，标准 .app / ad-hoc 签名，运行合成控制测试成功；与 Windows 同属 ed2aca9 M1 基线，外部 GStreamer runtime，未验证物理设备或正式发行。#45579708473 core CI 成功；因本批 README 更新触发了最新媒体 CI #45579865842，检查该 run 的三进程 WSS 结果。
