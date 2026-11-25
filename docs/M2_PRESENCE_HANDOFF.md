# 第十三批交接 — M2.2 身份、WSS 与设备上线

2026-11-25

## 已验证
- M1 Native media engine #45577251490、#45577702520 成功；跨平台原生输入 #45577702523 成功。前一批 Windows accepted socket 修复的 Linux 回归通过，不代表 Windows 完整运行测试通过。
- 开发包 #45577182682 仍执行（仅含 macOS rpath 修复），#45577251479 排队（包含 Windows blocking 修复与双端撤销）。两者均是 M1 基线，不含新 M2 界面。不得把旧包称为最新 master 或虚构下载。
- #45577702536 核心 CI 成功：共享协议、WSS client endpoint validation、持久身份抽象及内存后端测试。未实际访问用户凭据库。

## 本轮实现
- 91781d6：crates/rendezvous-protocol、crates/rendezvous-client，严格 URL、WSS 标准证书/hostname 校验、独立 server ID、禁止降级；device seed 持久存储 trait，Windows/macOS 可选 keyring 3.6.3 native-vault（独立于 LAN 临时会话密钥），本地文件锁、read-back verify、损坏拒绝，无私钥 IPC 或自动覆盖。
- f122a79：loopback WSS server（cert/key/server ID 三项齐全）、CLI register_device（显式 WSS 或本机 dev；私有 CA 通过 CLI 选定；可选 --persistent 需本机同意）。真实 TLS fixture 测试证书/hostname/server ID/重复注册及 heartbeat。
- 27d1c3a、d45b19b：桌面 PresenceManager 与服务器连接页、临时默认/持久显式勾选、在线 ID、心跳、拒绝请求数、主动下线、取消、退出、双语。上线预览最多5分钟，10秒 ping/5秒 pong deadline，不接受远程会话或系统输入。GUI 仅 WebPKI 公共信任根，私有 CA GUI 待办。
- c0091bb：修复 #45577808339/#45577983637 中 Tokio 拒绝 blocking listener 的 TLS 测试问题；在真实服务与测试入口设 nonblocking，并增加真实 presence 心跳、拒绝和取消回归。最新 #45578147785 Core CI、#45578147789 Desktop checks 记录时未结束。
- 系统凭据访问只在用户主动选择持久身份时开始，可能由 OS 弹出授权；取消之后阻止连接，但已经执行的本机凭据操作可能仍继续。实际持久存储及新桌面 GUI 未在物理设备验证。当前不配置自动上线。

## 接手
1. 先查最新 CI 与开发包，修复实际失败；当前 WSS startup test 仅已修正未确认结果，桌面新增依赖待编译。M1 开发包若成功提供真实 artifact 链接和 SHA，仍需用户授权的物理设备验收。
2. M2.2 收尾：持久信任设备记录与密钥变更确认、自定义 CA 配置、可靠设备身份及在线管理（包括重启、权限和断开）。
3. M2.3：基于共享协议接入真正的端到端会话 owner，绑定身份、grant、nonce、SDP、DTLS/SCTP，先经合成媒体和 inert backend 验证。GUI 当前拒绝全部来访，不允许自动授权。
4. M2.4 STUN/TURN、短期中继凭据、生产部署安全边界；M2.5 两网络实际验证。服务端目前仅 IPv4 loopback；未部署服务器、未改用户主机、未发行 Release。
