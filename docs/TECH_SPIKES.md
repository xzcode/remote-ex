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
