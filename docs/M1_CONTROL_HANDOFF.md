# M1 第九批交接 — 加密控制链路与原生控制原型

2026-11-25

## 上一轮已确认
- Desktop #45572297978、Media #45572298025、Native input #45572463267 全部成功。
- 本轮 Core CI #45574557958 成功（协议/授权/输入等 portable 核心）；媒体新集成 #45574557992 需继续检查。

## 本轮提交
- 8253f15：独立 lan_control 合成/inert 测试。Noise 加密信令、明确 VIEW|INPUT 授权、双方 SDP 签名、实际 video/SCTP DTLS 证书匹配，可靠通道接入 Dispatcher；心跳、停止、实际 tracked input release ACK。CI 模拟按住一个键后结束，要求 inert backend 记录释放。
- 7664893：可选 native-control feature，Windows/macOS Enigo/XCap，主显示器 geometry 与原生 minifb viewer，普通 OS 权限检查，按键/鼠标/滚轮、焦点与内容矩形、失焦释放、心跳、Ctrl-C/Escape、30秒限制。必须在双方可见交互终端独立核对完整 key bundle 且分别明确授权。GUI 仍仅支持只读，未接真实输入模式。未执行实际 OS 键鼠、屏幕采集。
- development-build workflow：官方 GStreamer 1.26.11 SDK（Windows x64/macOS native runner），编译 GUI、native-control 与 sidecars，inert 控制测试，产物打包上传 Actions artifacts。runtime 独立安装；macOS 仅 ad-hoc 签名、Windows unsigned。不是安装包或 release。详见 docs/DEV_BUILD.md。

## 未完成
- 获取新 media CI 和 development build 最终结果；修复编译/打包/运行失败，不跳过测试。新的 native-control feature 尚未经整体验证，GUI 屏幕权限/主屏 DPI/真实键鼠未实机验证。
- 正常停止有释放逻辑，但阻塞原生采集、异常进程 kill、系统拒绝释放仍存在风险；后续独立采集 worker、输入 watchdog、撤销反馈及实机故障测试。
- 将正式控制会话接入桌面窗口，使用优先 graceful revoke/stop，不把只读的 kill/EOF 策略复用为输入清理；实现可见持续同意/断开与更可靠焦点事件、坐标/布局、性能。
- 真实采集与输入权限、跨平台安装包、运行时依赖/许可证、锁定版本、安全审查仍需完成。无用户服务器改动，未部署任何服务或发布正式版本。M1 退出条件仍是两台物理电脑间授权远程控制闭环。
