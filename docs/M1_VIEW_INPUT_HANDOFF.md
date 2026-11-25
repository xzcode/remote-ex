# M1 第八批交接 — 真实只读桌面入口与输入适配

2026-11-25

## 已确认的运行证据
- Native media engine [#45571851093](https://github.com/xzcode/remote-ex/actions/runs/45571851093) 成功：同/跨进程 WebRTC，可靠模拟输入，以及 Noise 加密只读 synthetic profile 的实际 DTLS 证书校验与解码。
- Desktop checks [#45571374218](https://github.com/xzcode/remote-ex/actions/runs/45571374218) Windows/macOS 均成功；属于上一版本 GUI。
- 真实显示器和原生预览窗口未在物理机器测试，不声称远程控制已可用。

## 本轮提交
- 9d5ca6f：桌面 LAN Panel 新增真实屏幕共享（view-only）、加密测试、原诊断 profile；被控显示器枚举/明确选择、完整 128 hex key bundle、分别同意、可见提示、连接状态和断开；原 64 hex 合成诊断独立保留。固定 lan_view 可执行文件与 profile 参数，新增 supervised --desktop-session，批准后 stdin 持续保持，EOF/额外输入退出只读子进程；主窗口/应用退出仍 kill/wait。CI synthetic harness 使用监督模式，不采集屏幕。
- 4ec3de4：crates/input；portable Dispatcher 使用已验证的 SessionGate，串行消息检查、300/s 限流、归一化到授权显示区域、点击/滚动前指针位置校验、120单位滚轮换算、实际按下状态跟踪、失败/关闭/Drop 尽力释放。Windows/macOS 可选 Enigo 0.6.1 native 适配实现常用物理 USB HID 键、鼠标及滚轮，初期仅主屏，不支持所有键盘/IME/多屏/DPI 特例。新增针对性纯逻辑与原生编译测试；不会在 CI 触发真实键鼠。尚未接入任何现有网络会话或 UI 命令。
- 本轮不安装/部署服务，不改用户服务器，不发布安装包。

## 待验证
- [Desktop #45572297978](https://github.com/xzcode/remote-ex/actions/runs/45572297978)、[Media #45572298025](https://github.com/xzcode/remote-ex/actions/runs/45572298025)、[Core #45572463188](https://github.com/xzcode/remote-ex/actions/runs/45572463188)、[Native input #45572463267](https://github.com/xzcode/remote-ex/actions/runs/45572463267)，记录时未结束；先修真实失败。
- 真实共享仍依赖用户自行构建/安装可信 lan_view + GStreamer runtime，同目录 sidecar 未签名/自动打包，local-screen feature 和 macOS TCC、Windows屏幕采集、实际视频窗口与两台电脑仍待验。
- 只读30秒研发限制；连续截图采集方案并非最终性能方案。输入模块尚未具备真正的跨设备控制；突然终止进程不能保证释放，生产 owner 必须可撤销、正常清理并处理失败。不要把 Enigo 编译成功当作键鼠操作已验收。

## 下一批
1. 检查最新四类 CI；先完成 local-screen feature 的 Windows/macOS 原生构建和可执行开发产物打包，尽量降低手动安装门槛。
2. 真实只读路径在授权物理设备实测，完善侧车可信打包、原生窗口生命周期和权限。
3. 增加单独 INPUT 本机确认与受认证的控制 profile，验证实际视频及 SCTP DTLS 证书绑定，主控窗口采集输入并正确映射显示器内容区域；在有正常 OS 权限、可见同意、可断开情况下使用 Dispatcher，正常关闭必须先释放键鼠再停止。只读仍不启用输入。
4. M0/M1 的退出门槛不变；未实测时保持未完成。后续依赖锁定、许可、版本和打包验收。
