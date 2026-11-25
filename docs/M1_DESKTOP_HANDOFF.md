# M1 桌面客户端第二批交接

日期：2026-11-25

## 已确认的历史结果
- Core CI [45564676252](https://github.com/xzcode/remote-ex/actions/runs/45564676252) 成功，Rust protocol/core/security 测试通过。
- 本地容器有 Node/npm，无 Rust 和 GStreamer。首尔服务器上轮只做工具检测，本轮未修改服务器、未部署任何服务。

## 实现
- 提交 dee0ed57：独立 Tauri 2 + Vue 3/TypeScript 客户端，导航、远程功能尚未实现提示、单次本地屏幕诊断、偏好设置。
- 中英文、系统语言、跟随系统/浅色/深色/纯黑主题，localStorage 保存非敏感偏好；统一控件高度。后续增加系统语言动态变更与完整设置。
- XCap 0.9.10 显示器枚举及实际单帧采集调用；macOS 系统权限 preflight/request；其他平台依赖系统会话限制。
- 选定显示器后由本机用户手动点击采集。缩放至最大 1280×720，PNG 二进制 IPC；不保存、不上传、不后台采集。最大原始像素限制、单并发、频率限制、错误处理。退出诊断页/清除时释放预览 URL。此为诊断链路，不能当作连续视频方案。
- Desktop checks（Windows/macOS）与 Synthetic media probe（Ubuntu）已触发；真实编译结果以 Actions 为准。尚未生成安装包，bundle.active=false；当前图标为开发占位。
- 依赖版本仍未完整锁定，Cargo.lock/package-lock.json 待提交，传递依赖许可待审。

## 验收与下一步
1. 查看 [Desktop checks #45569412142](https://github.com/xzcode/remote-ex/actions/runs/45569412142) 和 [Synthetic media probe #45569412163](https://github.com/xzcode/remote-ex/actions/runs/45569412163)，修复编译/依赖问题并记录实测。
2. 按 apps/desktop/README.md 在已登录 Windows/macOS 真实机器测试显示器、授权、采集、清除、主题、语言。CI 无法代替实际权限与显示器验证。
3. 锁定依赖，完成原生连续采集适配，GStreamer RTP/WebRTC 双端传输，连续视频不走 Tauri 普通 IPC。先合成视频，再经本地同意的真实画面。
4. 接入身份确认、受限局域网信令、会话 gate、可靠数据通道和授权系统输入，断开释放键鼠。
5. M0 技术验证仍未完成，M1 正在开发；没有任何可连接公网、远程控制或无人值守能力。
