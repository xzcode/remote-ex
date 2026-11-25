# M2 端到端与跨网络验收

本文件是待执行清单，不是实测报告。2026-11-25；先完成 M1_DEVICE_ACCEPTANCE.md 的平台权限和控制验收。使用隔离测试设备、空白编辑器和明确用户同意，勿在生产/关键系统操作。

| ID | 项目 | 验收条件 |
|---|---|---|
| B01 | 核心信令及 TLS | 设备挑战注册、TLS CA/hostname/server ID 拒绝错误值、拒绝未准入设备、重放/错误签名/过期/角色拒绝 |
| B02 | 持久身份及可信设备 | Windows/macOS 重启后 ID 不变；拒绝损坏存储；无默认凭据库访问；独立核对 ID+key；pin 替换被拒绝，移除须确认 |
| B03 | WSS + 端到端协商 | 三进程 synthetic test，双方 Noise hash 与 Ed25519 role/context、原始 SDP 签名及实际 video/SCTP DTLS 通过；模拟输入释放，服务端只看到 sealed negotiation |
| B04 | ICE 直连 | 配置 STUN，记录实际候选类型与选中连接；两个授权网络（不同 NAT）可连接，P2P 优先，传输真实 view/control 仅在本机批准后 |
| B05 | 强制 TURN | 实际 coturn 短期凭据与 relay-only，双方仅 relay 候选、成功视频/输入/释放；两个网络复验，不能将同机 relay 测试视为跨网实测 |
| B06 | 直连失败/relay fallback | 禁止直连的测试网络，ICE All 应选择 relay；失效/过期凭据明确失败、重试仍需授权；记录实际候选对 |
| B07 | 服务器安全 | TLS/准入名单/防火墙、连接与队列限额、禁止匿名 relay、拒绝私网/管理/metadata 目标；来源限流及运营安全审查 |
| B08 | 真实只读/控制 | 两机选择相同 profile/relay policy；错误模式/错误 key/权限拒绝不开始真实画面；只读不执行系统输入；控制仅主屏/普通权限/聚焦窗口 |
| B09 | 断线与撤销 | 主控 Esc、双方 GUI 断开、WSS/媒体丢失、父进程退出时停止并记录实际释放；未确认释放必须提示；无自动权限升级 |
| B10 | 重连 | 可选 presence 重试同一身份最多3次；取消中止；错误证书/ID 不自动重试；真实媒体必须重新请求并经双方明确批准 |
| B11 | 构建及交付 | Windows/macOS 新 M2 开发包完整 CI 通过、实际启动；组件签名及 GStreamer runtime 核对；外部依赖/正式签名/许可证如实标示 |

## 记录模板

记录 commit SHA、Actions run、OS/arch、屏幕/DPI、GStreamer/coturn 版本、两端网络拓扑（不要记录实际个人连接信息）、是否 direct/relay、实际测试步骤和 PASS/FAIL/BLOCKED。记录必要的非敏感错误，不保存密钥、媒体、输入内容或 TURN 密码。M2 不允许仅凭核心单测或代码存在标记完成。当前未配置实际公网服务器或可供验收的两台授权图形桌面，物理测试待办。

## 第十六批：自动化验收实证（2026-11-25）

此记录区分自动化通过和真实设备/生产网络验收，不改变 B01–B11 的完整退出条件。

| 项目 | 实际状态 | 证据或剩余项 |
|---|---|---|
| B01 | 自动化 PASS | Core CI #45584030066：真实 TLS 注册、错误证书/主机名/server ID、错误注册证明、准入、会话隔离及顺序、有效期 |
| B02 | 部分 PASS | 持久身份内存后端重读与损坏检查通过；Windows/macOS 系统存储重启、SQLite trust 实际平台测试仍待 |
| B03 | 自动化 PASS | #45584030116：真实本地 WSS 服务及两个媒体进程；30 帧解码、签名 SDP、实际 video/SCTP DTLS、inert input release |
| B04 | 部分 PASS | 同一 CI 主机 direct ICE 实际媒体成功；不同物理 NAT/公网直连与选中 candidate pair 未实测 |
| B05 | 部分 PASS | #45584030116：真实 coturn 短期凭据 + 强制 relay，30 帧解码与释放回执；不同物理网络待 |
| B06 | 待验证 | bb65000 新增隔离双 NAT 测试：网关仅允许访问 WSS、TURN listener，ICE All 自动 fallback 与 Relay 两种路径；#45585068195 待结果 |
| B07 | 部分 PASS | 实际 TLS/allowlist 与协议边界测试通过，部署配置未执行；网络隔离、正式运营和防滥用审查仍待 |
| B08 | BLOCKED | 当前无已授权两台物理 Windows/macOS 图形桌面，未读取实际屏幕或执行实际键鼠 |
| B09 | 部分 PASS | LAN synthetic 正常结束、双方监督管道撤销，inert backend 释放验证；routed direct/relay 正常结束与 ACK；真实路由撤销/焦点/权限未实测 |
| B10 | 自动化 PASS（在线恢复） | #45584030066：实际 WebSocket 断开后同一设备身份重新注册；不恢复系统输入授权 |
| B11 | 未完成 | 旧 M2 package #45583806921 两平台因 SQLite 类型错误失败；e2869fc 已修，#45584544562 桌面重验、#45585068172 最新 M2 完整开发构建待结果。M1 旧包仍可用，但不含 M2 |

双 NAT lab 只在明确 opt-in 的 GitHub-hosted Linux Runner 上运行。独立 namespaces 中建立 lab bridge、两个 NAT gateway 与两个 peer namespace，禁止直接媒体流和外网路径；只允许 WSS/turn listener。临时测试证书、密钥、准入名单和 relay secret 均为当次生成，不写入仓库。实际服务通过 explicit TLS + allowlist bind 启动，诊断程序仅合成视频与 inert input。全部 namespaces 和进程在测试后清理。该测试仍是单一虚拟机内的隔离网络实验，不是跨公网、物理设备验收。

验收批次代码源 bb65000d467ef893683aae441602d2512d087b2e。没有启动或修改用户服务器，没有部署或发行。以下 CI 在记录时尚未结束，必须检查实际结论：
- [最新核心 #45585068192](https://github.com/xzcode/remote-ex/actions/runs/45585068192)
- [双 NAT/媒体 #45585068195](https://github.com/xzcode/remote-ex/actions/runs/45585068195)
- [桌面重验 #45584544562](https://github.com/xzcode/remote-ex/actions/runs/45584544562)
- [新 M2 开发包 #45585068172](https://github.com/xzcode/remote-ex/actions/runs/45585068172)

