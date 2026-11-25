# Remote EX

跨平台、可私有化部署的远程桌面工具。Remote EX 自主研发客户端、服务端和业务协议，使用成熟底层库，不基于 RustDesk 等完整产品进行 fork。

**当前状态：M0 技术验证待完成；M1 Rust 协议、身份及授权核心已编写，尚未编译验证；无可运行远程桌面客户端或发行包。**

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

## 选型

Rust + Tauri 2 + Vue 3/TypeScript；WebRTC/ICE/STUN/TURN；Rust/Axum 信令服务；coturn 中继；SQLite。GStreamer 是媒体引擎首选候选，待跨平台原生渲染与打包验证后冻结。许可证待所有者决定及第三方依赖审查，公开仓库不自动代表已授予开源许可。

## 安全

仅用于有权限管理的设备。默认不开放无人值守、不绕过系统授权、不隐藏连接状态，不允许未授权键鼠、文件或剪贴板访问。生产凭据不得进入仓库。

