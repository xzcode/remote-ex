# M1 第七批交接 — 加密只读屏幕共享原型

2026-11-25

## 已验证
- 上一版媒体 #45571374184 成功（合成视频与测试数据回环）。
- Core CI #45571614512 成功：新增 Noise 双向固定公钥的加密 TCP 往返与错误对端测试。
- 上一版桌面 #45571374218 记录时仍在运行，需核对最终结果。

## 新增实现
- 63db325：Snow 0.10 Noise KK encrypted signaling、bounded records、超时/失败关闭、实际 GStreamer video DTLS peer certificate 与签名 SDP fingerprint 的 SHA-256 匹配及测试。
- 8a23d71：独立 lan_view（--test synthetic；--screen DISPLAY_ID / --view explicit real mode），临时 Ed25519+X25519 双公钥 bundle、可信独立核对、交互同意、固定 VIEW-only grant、加密 profile/签名/ICE/Ready/heartbeat、单显示器 XCap capture feature、黑边等比缩放、native RGBA appsrc 与 native controller video sink。真实画面只在两端身份、DTLS 实际证书和许可验证通过后进入 pipeline。最长30秒，普通 OS 权限，不接受真实键鼠输入。
- scripts/test_secure_view.py：两个独立进程的 synthetic-only encrypted integration；检查解码帧与 DTLS 验证成功，不读取真实桌面。CI 新增此项。
- 同批调整：主机停止采集后等待控制端结束消息再停止 pipeline，避免最后一帧被提前断开。
- 真实模式必须交互终端；暂未接入 Tauri 主窗口、未签名/打包媒体依赖、未验证 local-screen feature 的完整原生构建或跨机 GUI。未部署服务、未修改用户服务器。屏幕采集采用逐帧 XCap 诊断路径，10fps/960×540 仅研发基线；非生产性能。

## 下一批
1. 获取最新 core/native/desktop CI 结果，修复实际失败；重点核对 GStreamer DTLS transport property 与证书比对是否能在真实连接完成。
2. 通过可信物理 Windows/macOS 两台电脑测试真实显示器与原生窗口，检查 macOS TCC 权限，确认关闭终端、连接中断或超时后停止。不要在未授权设备采集。
3. 完善正式只读会话 UI、已签名媒体组件打包与实时状态/断开、持久身份；完成授权控制输入的 dispatcher 与平台适配，独立同意、撤销和输入释放。真实系统输入尚未实现。
4. 锁定 Cargo/npm 与原生 GStreamer 版本，确认许可证、媒体平台分发。M0/M1 仍未通过完整退出门槛。

运行步骤见 crates/media/README.md。未发布任何安装包。
