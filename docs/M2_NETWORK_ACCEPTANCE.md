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
