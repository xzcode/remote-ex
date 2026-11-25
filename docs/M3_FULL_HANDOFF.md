# M3 十项功能开发交接（2026-11-25）

状态：功能候选，不是完整验收/正式发行。基线 M2 bb65000 的核心、媒体/双 NAT、两平台开发包自动化成功，真实设备仍待验收。M3 本次未访问用户服务器、未执行真实 OS 输入/采集、未发布 Release。

| 工作包 | 已编写并推送 | 剩余验收/开发 |
|---|---|---|
| M3.1 | SQLite 可信设备分组/备注/收藏/搜索/筛选；可选 200 条本地历史与清空；最多32个已保存可信 ID 的显式在线/忙碌查询、限流/过期 | 持久化与 GUI 实机；历史目前覆盖 routed test/view/control |
| M3.2 | REX1 URL-safe Base64 JSON 邀请、5分钟过期、严格 WSS URL/服务器ID/公钥指纹核对、导入/生成 | 邀请不是密码或签名授权；短码分配服务尚无 |
| M3.3 | 可调整尺寸的原生等比窗口、黑边/焦点输入映射、标题与 Esc 终止 | 真全屏、会话内完整工具栏、真实 DPI/焦点 |
| M3.4 | 480p/540p/720p/1080p/1440p/2160p 预设；双方加密协商；配置码率/帧率；基于报告丢包的有界码率调节 | 运行中分辨率/帧率切换、真实性能/自适应与高分辨率压测；4K 仅目标配置 |
| M3.5 | 本地枚举，被控端选择具体显示器；只读允许非主屏 | 实时换屏/多屏同看、非主屏控制坐标验证；控制仍限主屏 |
| M3.6 | Win/macOS arboard 文本/PNG 单次收发，独立 kind/方向/双方授权、容量与图像解码限制 | 真机读写剪贴板；不含自动持续同步 |
| M3.7 | 双向可选单文件、源/新目标文件选择、64 MiB、SCTP 有界窗口、SHA256、禁止覆盖、进度、当前会话 sender 暂停/继续 | 两方向传输及暂停端到端 CI；拖拽/目录管理/断开后续传尚无 |
| M3.8 | 独立音频会话；Windows WASAPI loopback、Win/macOS 播放、本地音量；合成 16kHz PCM 验证脚本 | 真实系统输出采集/播放，macOS 系统采集、音视频同会话、Opus/A-V sync 尚无；不以麦克风替代 |
| M3.9 | 三个独立主控槽位、tab/独立原生窗口、退出清理 | 真实并行验收；服务仍拒绝同一设备ID重复在线，需独立登记身份；host/file/clipboard/audio 独占 |
| M3.10 | GStreamer get-stats 数值采样、RTT/丢包/码率估算、选中 direct/relay、解码 fps/像素吞吐分开；不报告未知数据为真实值 | 实际统计字段/精确性、运行与 UI 核对，性能评审 |

## 代码索引

- b2af51b：desktop-features 邀请、质量、内容校验。
- 6b3b824 / 5ce57f9：设备资料、历史、桌面管理。
- a1b077a：质量协商、窗口、显示器选择。
- 3e34c8b / 1f4db94：独立文件/剪贴板传输协议、界面、双向测试。
- 79818da：独立音频、系统回录/播放候选与 synthetic test。
- 5e7078b：多会话、统计。
- 341654b：可信设备在线状态查询。
- b244dc4：文件选择器与当前会话暂停/继续，父进程固定命令/EOF 撤销。

## 实际验证

- Core #45595602432 (341654b) success。Native input #45595220278 success。
- #45595602528 (341654b) 在后续提交取消前，媒体单测、两种回环、独立 LAN、加密只读/模拟控制、三进程 WSS、实际 TURN forced relay 步骤 success；双 NAT M3、本轮文件/音频步骤未完成，不将取消标为整体验收。
- #45595602968 (341654b) 取消前两平台前端 build success，macOS Rust check success；不是最新完整桌面验收。
- 后续提交触发最新 core、desktop、media，以及包含 native-control、文件暂停与 synthetic 音频的两平台开发包。核对实际 run SHA 与结论，失败按日志修复。不能提前宣称通过或已生成 M3 包。

## 安全与验收

继续保持双方完整公钥验证、独立 profile/role/grant、端到端 sealed 协商、DTLS 和撤销；文件只读本地明确选定的普通文件，接收指定新路径，临时文件内容验证后不覆盖提交，远端不能选择本机路径。剪贴板不监听；音频不采集麦克风；不自动续接权限。输入/音频 30秒、传输 75秒实验上限。M3 需要两台明确授权的 Win/macOS 设备、系统录屏/辅助权限、不同网络；没有实际环境则标 BLOCKED。保留 M1/M2 物理验收缺口。不得把代码存在或合成测试当成全部工作包完成。

## 第二轮收尾（2026-11-25）

### 实际上一批结果
- [#45596398523](https://github.com/xzcode/remote-ex/actions/runs/45596398523)：f468122 Windows/macOS 桌面 CI 全部通过。
- [#45596398402](https://github.com/xzcode/remote-ex/actions/runs/45596398402)：双 NAT 回退与强制中继成功；双向实际 65,792 字节文件传输、暂停/恢复和 SHA256/接收内容一致通过。独立合成音频启动失败 `Visible local consent required`，测试未带 `--desktop-session`；没有得到 PCM 成功证据。
- [#45596398446](https://github.com/xzcode/remote-ex/actions/runs/45596398446)：两平台完成原生 Release 编译；macOS 文件暂停/恢复通过，随后同音频 harness 错误失败；Windows 文件暂停测试因控制行 CRLF 不匹配触发 CANCEL，旧包失败，未上传 M3 artifact。

### 新增提交
- a9576f0：合成音频 harness 增加明确监督模式，不取消授权检查。
- b833e9d：独立 routed viewer（Winit 0.30.13 + Softbuffer 0.4.8 + font8x8 0.3，须复验平台依赖及许可证）；等比缩放、borderless fullscreen/F11、本地工具栏断开/暂停输入、物理像素输入映射、局部 UI 与黑边不转发输入，LAN 窗口不变。
- af1076f：host 事先明确选最多四块显示器，只读 profile 才发送加密 ViewLayout；本地工具栏 Next screen 通过固定消息在预选列表及拼接布局间循环，host rate limit，实际帧推送后确认布局；只读不实例化 OS 输入。控制仍限主屏。
- d41fe73：固定有限 supervisor LF/CRLF 命令兼容；EOF/未知指令仍撤销；native 首张真实授权帧成功后才 READY_ACK；本地音频播放实时音量。
- 88b5c1f：修正错误的 viewport 测试点（#45598935496/#45599135499 失败原因），从实际 fit 矩形取中心，断言 toolbar/black bars/content boundaries；[#45599302773](https://github.com/xzcode/remote-ex/actions/runs/45599302773) core success。
- fe04f85：系统凭据分槽位持久身份，第一槽位保留 M2 key，另两槽位显式准备预登记；不自动访问系统凭据、不覆盖已有 key、不改变服务端重复同 ID 拒绝策略。

### 最新测试与接手
- 最新代码基线 fe04f85：[Core #45599426885](https://github.com/xzcode/remote-ex/actions/runs/45599426885) / [Native media #45599426956](https://github.com/xzcode/remote-ex/actions/runs/45599426956) / [Desktop #45599427074](https://github.com/xzcode/remote-ex/actions/runs/45599427074) 记录时运行中。较早 runs 因新提交取消，不视为成功。
- DEV_BUILD.md 触发新完整 Windows/macOS development build，查最新 run 的 SHA 和原生编译、文件/音频测试证据，再给实际 artifact 下载链接。新 Winit/Softbuffer 窗口原生功能还未实机。优先修复构建、协议或测试问题，不重复频繁取消 CI。
- 未完成：同 ID 复用多会话（当前3个独立稳定身份）、非主屏控制、多屏独立窗口、跨连接文件续传、远程目录/拖拽、持续剪贴板、macOS system-audio、A/V 同步、真实系统权限及持续连接/性能测试。用户要求推进 M3，不把未实现项或 CI pending 当完成。未改用户服务器、未发布正式 Release。
