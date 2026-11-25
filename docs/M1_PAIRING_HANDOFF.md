# M1 第四批交接 — ICE 修复、原生帧与本地配对

日期：2026-11-25

## 已确认的历史结果
- Windows/macOS Desktop checks [45569412142](https://github.com/xzcode/remote-ex/actions/runs/45569412142) 完整通过。
- Core CI [45569930118](https://github.com/xzcode/remote-ex/actions/runs/45569930118) 通过（第三批）。
- Synthetic media probe [45569412163](https://github.com/xzcode/remote-ex/actions/runs/45569412163) 通过。
- 原生媒体 [45569853979](https://github.com/xzcode/remote-ex/actions/runs/45569853979) 编译及 config_bounds 通过，但实际 WebRTC loopback 失败；日志报 Invalid ICE candidate or peer state。不是编译或插件缺失问题。上游 https://gstreamer.freedesktop.org/releases/1.24/ 说明 empty candidate 是合法完成标记，现已修复该可能根因，运行结果必须复验。

## 代码
- 55bdc8ac：ICE completion 支持、边界与字符检查、原生合成 RGBA 输入回环、可选 autovideosink 独立窗口、local-screen feature 的交互式本地真实屏幕诊断。真实屏幕只在明确本地确认且两个本进程 peer Connected 后读取；最多30秒，未部署外部信令或屏幕接收服务。实际画面路径尚未编译验证（可选 feature 默认关闭），渲染也需实机。重复截屏/缩放仅诊断，未优化，可能变形；不能宣称正式连续采集。
- 4f11ff38：PendingPairing/ApprovedPairing/SignedPairing，一次性状态、随机 nonce/ID、120秒过期、双方一致授权和 SDP 证明，SessionGate 强制匹配签名证明与 session/grant；新增重放、超时、授权不一致测试。实际网络身份、DTLS绑定、UI授权、信任记录尚未实现。
- 最新媒体 CI [45570342512](https://github.com/xzcode/remote-ex/actions/runs/45570342512) 运行中；最新配对核心 CI 待查。未发布安装包、未修改服务器。

## 下一批优先
1. 获取最新 media/core CI 结果并针对失败修复；两种 loopback 均需真实解码帧通过，不能跳过。
2. 在实际 Windows/macOS 环境编译 local-screen feature 与 GStreamer 运行库、验证原生预览及正常系统权限。
3. 设计最小 LAN 信令 wire schema（有界/超时/一次性请求），真实双端本地许可与可信全指纹确认；认证签名交换、实际 DTLS 关联后才能切换真实帧及接受数据通道。
4. 接入 SessionGate 与可信会话 owner，建立可靠输入数据通道、平台输入执行和断开清理。
5. 原生连续采集与 GPU 渲染优化、依赖锁定及许可证审查。M0/M1 仍未完成。
