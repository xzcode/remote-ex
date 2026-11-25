# Remote EX

跨平台、可私有化部署的远程桌面工具。Remote EX 自主研发客户端、服务端和业务协议，使用成熟底层库，不基于 RustDesk 等完整产品进行 fork。

**当前状态：M0 技术验证待完成；基础桌面 CI、核心、同进程 WebRTC、双进程合成视频与可靠测试输入通过；加密 LAN 信令核心通过；新增真实屏幕只读 CLI 原型待集成及实机验证。尚无实际系统远程控制或发行包。**

## 目标

- Windows、macOS、Linux 同一客户端兼任主控与被控，优先实现 Windows/macOS。
- 局域网可独立运行；公网优先 ICE/P2P，失败时通过自建 TURN 中继。
- 不强制账号、不依赖公共服务；显式授权、会话可见、可随时终止。
- 多会话、剪贴板、文件、音频、多显示器、无人值守、语言与主题逐步实现。
- Android/iOS 主控和浏览器端列入 M6，不阻塞桌面首版。

## 开发文档

- [产品需求与平台范围](docs/PRD.md)
- [技术架构与选型决策](docs/ARCHITECTURE.md)
- [会话协议与安全规范](docs/PROTOCOL_SECURITY.md)
- [UI 与交互规范](docs/UI_SPEC.md)
- [里程碑与验收计划](docs/ROADMAP.md)
- [M0 技术验证与依赖审查](docs/TECH_SPIKES.md)
- [开发代理约定](AGENTS.md)
- [M0/M1 第一批交接](docs/M0_M1_HANDOFF.md)
- [M1 第二批交接](docs/M1_DESKTOP_HANDOFF.md)
- [M1 第三批交接](docs/M1_MEDIA_HANDOFF.md)
- [M1 第四批交接](docs/M1_PAIRING_HANDOFF.md)
- [M1 第五批交接](docs/M1_LAN_HANDOFF.md)
- [M1 第六批交接](docs/M1_DESKTOP_LAN_HANDOFF.md)
- [M1 第七批交接](docs/M1_SECURE_VIEW_HANDOFF.md)
- [LAN 信令诊断](crates/signaling/README.md)
- [原生媒体模块说明](crates/media/README.md)
- [桌面开发说明](apps/desktop/README.md)

## 选型

Rust + Tauri 2 + Vue 3/TypeScript；WebRTC/ICE/STUN/TURN；Rust/Axum 信令服务；coturn 中继；SQLite。GStreamer 是媒体引擎首选候选，待跨平台原生渲染与打包验证后冻结。许可证待所有者决定及第三方依赖审查，公开仓库不自动代表已授予开源许可。

## 安全

仅用于有权限管理的设备。默认不开放无人值守、不绕过系统授权、不隐藏连接状态，不允许未授权键鼠、文件或剪贴板访问。生产凭据不得进入仓库。

