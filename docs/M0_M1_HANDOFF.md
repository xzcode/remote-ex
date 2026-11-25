# M0/M1 交接 — 第一批

2026-11-25

## 实际提交内容
- Cargo workspace：protocol、security、core。此批不是完整 Tauri 应用。
- Protobuf v1 有界输入消息与 schema；所有输入在可靠有序通道上传输；仅局部输入解析，不是完整信令协议。
- Ed25519 设备身份与注册挑战签名/验证原语；未实现密钥持久化、服务器挑战生命周期、首次配对或 DTLS 指纹绑定。
- 被控会话本地授权门控、序列校验、键鼠释放计划；尚未接入 OS 注入及真实认证传输。on_verified_transport 是受信任内部集成入口，绝不可由网络消息直接调用。注入失败和进程退出清理由后续平台 dispatcher 负责。
- Python GStreamer 合成画面 VP8 编解码诊断；只测试合成视频，不读取真实桌面、不开放网络服务。插件发现不等同 WebRTC 已验证。

## 验证状态
本次检查本地 Linux 环境和授权首尔服务器，均未找到可直接使用的 rustc/cargo 或 GStreamer 工具。未修改首尔系统、未安装工具链、未部署服务。Rust 构建、依赖锁定、媒体 pipeline 和真实设备测试均待执行；代码不能宣称已经可运行。新增最小 CI 工作流在 Ubuntu 上执行 cargo test；结果以 Actions 实际日志为准。当前版本范围仅初始候选，不代表安全审计或依赖许可冻结。

## 操作
在具备 Rust stable 的环境运行：
```sh
cargo test --workspace
cargo fmt --all
```
在安装 GStreamer 及相应插件的环境运行：
```sh
python3 scripts/probe_media.py
python3 scripts/probe_media.py --preview
```
第一条媒体命令无需 GUI；第二条只显示本机合成测试图案。输出 JSON 记录版本、元素与运行状态，不代表端到端延迟指标。

## 后续顺序
1. 编译 core 并补 Cargo.lock；按结果修复，而不是跳过。
2. GStreamer 合成画面验证，再完成双端 WebRTC 媒体 spike、原生渲染、Windows/macOS 权限验证。
3. 建立 Tauri/Vue 桌面骨架，接入本地诊断和系统权限；真实采集/编码/解码。
4. 实现身份握手和权限 UI 后再接入可靠数据通道、平台输入 dispatcher；完成真实双端控制。
5. M0 仍未通过技术退出门槛；M1 是核心代码并行推进，不能标为完成。
