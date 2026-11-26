# M6 第一批交接

基线：alpha.2 已实际发布于 2026-11-26 14:32:30（UTC+8），tag 指向 6a42d6e。七项 CI 与发布检查全部通过，六个安装/便携包加两个校验资产。新开发代码不在该 Release 中。

| 工作包 | 本批实现候选 | 未完成 |
|---|---|---|
| M6.1 | 28 种 v1 profile 共用定义，签名校验与媒体 grant 复用，离线 JSON 合约，Rust/Node WebCrypto 固定公开向量 | Noise 实际跨端握手、真实 DTLS、完整互操作 |
| M6.2 | 本地 allowlist 检查、新建、录入/撤销计划、差异与 canonical SHA-256；只写新文件不覆盖；启动共用验证 | 在线管理、聚合运行状态、实际部署 |
| M6.3 | 最多 8 个 TURN，管理员区域筛选、优先级、同优先级 session 稳定分流；两端同 relay，各自短期凭据 | 健康测量、故障切换、多 NAT 多 relay 实测 |
| M6.4 | Node WebCrypto 独立签名/哈希验证候选 | 实际浏览器 Noise/WASM、WebRTC 与密钥存储 |
| M6.5–10 | 规划保持 | 未实现 |

M6 不改变旧 v1 消息或既有 profile 权限。管理命令无网络、自动应用、重启或授予桌面访问权限；撤销计划需要管理员实际部署与重启后才生效。TURN pool 只对新 source build 生效；无公网管理端口、无真实 endpoint/credential、无自动 DNS 探测。实际 DNS/防火墙/证书和 relay 隔离仍需管理员复核。核心及新自动化结果待确认，未标记 M6 验收完成。下一批优先补实测跨端 Noise/WebCrypto 与管理运行状态安全设计，再推进浏览器合成媒体候选。旧 Release 保持不变。


## First batch verification

02031a8 all six applicable runs passed: Core 45770433146, Desktop 45770433415, Native input 45770433231, Native media 45770433201, Linux 45770433259, Container 45770433169. New browser synthetic lab is documented in M6_WEB_LAB_HANDOFF.md; real cross-network relay health/failover and full client interoperation remain unverified.
