# M1 第十一批交接 — 双端正常撤销、开发包复验

日期：2026-11-25

## 已确认
- 上轮 0b3b758 的 Native media engine #45575600914 全部成功，包括加密控制、模拟输入与释放回执。
- Desktop checks #45575817861 的 Windows/macOS 前端及原生 cargo check 已通过；最终单测记录时仍执行中。
- Desktop development builds #45576059718 已通过 Windows/macOS GStreamer 1.26.11 SDK 安装和检测，双方已进入桌面 release 构建。旧 SDK 路径问题已解决。尚未取得最终 artifacts，不宣称包已生成。

## 本轮代码
- 6a7780d：本机被控端撤销与远端 STOP 均先停止新输入、Dispatcher::close，随后发送释放 ACK。主控轮询 ACK，收到时停止输入窗口，不再继续发送输入。主控本机撤销后走 STOP/释放 ACK。网络失败或 OS 释放失败仍只能报告未确认。
- scripts/test_secure_control.py 使用真实独立进程、Noise/DTLS/SCTP、inert backend；普通结束需要至少30解码帧，另两种用例等待 inert key 按下后关闭 host 或 controller 的监督 stdin 管道，要求两端释放确认、测试 backend 实际释放检查及正常退出。不会读取真实屏幕或执行系统键鼠。
- 新 Core CI #45576282415 与 Native media engine #45576282390 记录时未结束，新增回归尚待复验。
- docs/M1_DEVICE_ACCEPTANCE.md 明确两台真实设备的验收工作，不以编译/合成输入通过替代实机。

## 接手
1. 优先检查 #45576059718 已运行的开发包，不取消已开始的跨平台编译。该构建锁定在 367cb10，不含6a7780d撤销增量；即使成功也只能标为前一版本的开发包。取得实际失败日志或 artifact 后按情况修复，并在新媒体 CI 通过后重新构建最终版本。
2. 核对 #45575817861、#45576282390、#45576282415；如新增双端撤销测试失败，按真实日志修复。
3. 完成 macOS .app 与 Windows 测试包启动及媒体运行时整合，依赖锁定、许可、签名仍待。开发包当前外部 GStreamer runtime 方案。
4. 使用 M1_DEVICE_ACCEPTANCE.md 进行授权物理设备测试。当前只有 CI 环境，没有已授权的 Windows/macOS 图形桌面可做实际验收。暂保留主屏/30秒/私网限制，不进入无人值守或生产公网部署。
5. 未修改用户服务器、未部署服务、未发布正式 Release。M1 仍未完整验收。
