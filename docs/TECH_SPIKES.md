# M0 技术验证与许可证

状态：所有 spike 均待执行；此处为选型门槛，不包含实验结果。

| 编号 | 验证内容 | 通过门槛 |
|---|---|---|
| S01 | GStreamer WebRTC 双端媒体 | 真正的采集→编码→发送→解码→渲染；优先 1080p30 |
| S02 | 原生渲染集成 | 独立渲染窗口或原生嵌入；缩放/多 DPI 正常；无逐帧普通 Tauri IPC |
| S03 | 平台权限与输入 | Windows/macOS 各自正规授权、普通桌面键鼠、权限拒绝明确处理 |
| S04 | 打包 | Windows 与 macOS 原生 runner 能构建、安装及运行最小客户端；Linux 依赖清单明确 |
| S05 | ICE/TURN | 本机与两网络场景，UDP 直连及指定 TURN；可测实际候选类型 |
| S06 | 许可证清单 | 锁定依赖版本、SPDX、来源、动态/静态链接、插件/编码器与二进制分发义务 |
| S07 | 安全握手 | 对端确认、媒体指纹绑定、拒绝身份替换/重放的设计评审与针对性验证 |

## 媒体选择标准
首选 GStreamer；如原生渲染、硬件编码、发行包体积或依赖分发不可接受，比较 libwebrtc 与 webrtc-rs+原生媒体库。评分维度：Windows/macOS/Linux 可构建性、硬件支持、互操作、弱网、部署、可维护性、许可。不可因单平台 demo 成功就确认全平台支持。

## 依赖许可初始记录（需核对锁定版本）
- Tauri / Rust 生态、Vue、Axum：核对具体依赖 SPDX 及 NOTICE。
- GStreamer 核心为 LGPL 系列；插件可能链接不同许可的外部库，按实际安装/打包内容审查。参考 https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/licensing.html
- coturn BSD 类许可；保留许可证与声明，核对锁定版本。参考 https://github.com/coturn/coturn/blob/master/LICENSE
- H.264/H.265/AV1 等专利及编码器分发要求分别检查；项目开源许可证不自动消除第三方义务。
- 最终项目许可证由所有者确认；不添加未经确认的 LICENSE，不采用其他完整远程桌面项目的源码作为基础。

## 记录模板
编号、日期、提交、环境（OS/CPU/GPU/版本）、命令/步骤、结果、日志/性能数据、限制、选择或淘汰理由。若无法获取平台环境，记为待验证。

## 2026-11-25 增量记录

Rust 核心 CI 通过：[45564676252](https://github.com/xzcode/remote-ex/actions/runs/45564676252)。这不是媒体测试。桌面候选使用 XCap 0.9.10 的 Monitor API 与 Tauri 原生命令做单帧诊断；暂不宣称连续采集、真实桌面权限或 GPU 渲染完成。XCap 上游声明 Apache-2.0，实际传递依赖与媒体分发义务仍需审查。Windows/macOS Desktop checks 与 Ubuntu Synthetic media probe 已触发，结果见 Actions。S01–S07 退出条件不变。

## M1 第三批实际记录

- [Synthetic media probe #45569412163](https://github.com/xzcode/remote-ex/actions/runs/45569412163)：成功，合成 VP8 编解码及所需 WebRTC 元素检测通过；不是双端 WebRTC 结果。
- [Desktop checks #45569412142](https://github.com/xzcode/remote-ex/actions/runs/45569412142)：macOS job 成功，Windows 前端和 Rust check 成功，Windows 最后测试尚未完成（记录时）。无真实桌面权限/采集证据。
- [Native media engine #45569853979](https://github.com/xzcode/remote-ex/actions/runs/45569853979)：运行中；本批 Rust GStreamer WebRTC 引擎的编译和双端解码结果仍待确认。S01 尚未通过完整退出门槛。
- 新增 GStreamer Rust binding 版本范围 0.23，原生 GStreamer 最低 1.20；锁文件、插件许可证与三平台分发仍待审查。

## M1 第四批实际记录

- Desktop checks #45569412142：Windows 与 macOS 均成功（编译/单元测试，非实机采集）。
- Native media engine #45569853979：GStreamer 1.28.2 + gstreamer-rs 0.23 编译及 config_bounds 通过；loopback 运行失败，日志 `Invalid ICE candidate or peer state`。上游 1.24 release notes 明确空字符串候选表示 ICE 结束，修复该兼容性并增加边界测试及原生 RGBA 输入验证。新 run #45570342512 未结束，不能宣称已解决运行失败。
- local-screen 与 autovideosink 是待平台验证的诊断实现，不等同跨设备屏幕分享或持续 GPU 采集。S01–S07 退出门槛不变。

## M1 第五批实际记录

- Native media engine #45570342512 成功：修复 ICE 完成标记后，两种同进程 WebRTC 测试均通过（原生 VP8 测试图和 RGBA appsrc 测试输入，各至少 30 解码帧）。这是同一 CI 主机验证，不是跨设备或公网。
- Core CI #45570749829 成功：新增信令序列化/边界/真实 loopback TCP framing 测试通过。
- c21fda1：新增可靠 SCTP 数据通道和两独立进程诊断，运行 #45570886138 及 Core CI #45570886109 待最终结果；未宣称新集成通过。
- GStreamer webrtc binding 启用 v1_22，以获得 send_data_full；实际依赖锁定、Windows/macOS 原生媒体包及 GPU 渲染仍待验证。

## M1 第六批实际记录

- Native media engine [#45570973647](https://github.com/xzcode/remote-ex/actions/runs/45570973647) 成功：两个独立进程的本地 TCP 信令、双方 SDP 签名、WebRTC 视频、可靠有序 SCTP 及固定测试输入回执；同进程两个回环仍通过。初次 #45570886138 被后续提交取消，由此 run 替代。不是两台真实机器的测试。
- Core CI #45570973677 成功。新桌面监督器、UI 与 preview 状态增量代码仍待最新 CI；不把历史桌面编译结果当作本批成功。
- macOS/Windows GStreamer 运行时打包、原生显示窗口、跨设备真实屏幕和系统输入仍待验证。

## M1 第七批实际记录

- Native media engine #45571374184 成功：上一轮 CONNECTED/preview 参数增量后同/跨进程合成回环通过；不验证 GUI preview。
- Core CI #45571614512 成功：新增 Noise 双向预先固定公钥加密往返与错误对端测试通过。第七批 screen profile/实际 DTLS 属性校验及双进程测试需看最新 Native media engine。
- 真实屏幕 CLI 依赖 local-screen feature，默认 headless test 不读取屏幕。该 feature 与跨平台 native rendering 仍待物理设备确认；测试原型 960×540/10fps，非性能指标。

## M1 第八批实际记录

- #45571851093 Native media engine 成功：前一批 Noise 加密只读 profile 的双进程 synthetic test、实际视频 DTLS remote certificate 与已签名 SDP 的验证均通过；没有读取真实屏幕。
- #45571374218 Desktop checks Windows/macOS 完整通过（第六批桌面监督器）。
- 9d5ca6f：新增 desktop-session 与只读 UI；Core CI #45572297985 通过，媒体 #45572298025 与桌面 #45572297978 记录时未结束。真实 screen feature、OS permission 和原生 GUI 待实机。
- 4ec3de4：新增 input portable dispatcher 与可选 Enigo Windows/macOS native adapter；Core CI #45572463188 及 Native input checks #45572463267 待结果，测试不注入系统事件。Enigo 上游 https://docs.rs/enigo/0.6.1/enigo/；依赖及许可证仍待审查。

## M1 第九批实际记录

- #45572463267 Windows/macOS 原生输入适配编译与 inert 测试全部通过，#45572297978 桌面增量 CI 与 #45572298025 媒体集成 CI 成功（上一轮）。
- #45574557958 Core CI 成功（本轮控制几何协议与原有单测）；Native media engine #45574557992 仍需核对新 encrypted control/inert integration 的结果。
- 开发构建使用官方 GStreamer 1.26.11 SDK，Windows MSVC x64 MSI、macOS universal PKG。CI 校验官方 SHA256 文件并构建 native-control；尚未证实跨平台 native-control 整体编译或包生成。不得将 inert 测试当作 OS input/真实桌面验收。
