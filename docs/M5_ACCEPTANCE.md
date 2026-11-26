# M5 验收与 alpha.2 发布交接

日期：2026-11-26。当前为 alpha.2 预发行候选，**不代表 M4/M5 实机验收完成或稳定版发布**。

## 一、已经取得的自动化证据

| 源提交 | 自动化检查 | 结果 |
|---|---|---|
| 89ba615 | [Windows/macOS development builds](https://github.com/xzcode/remote-ex/actions/runs/45749483630) | 通过：Windows 当前用户安装/卸载及运行环境探测；macOS DMG 挂载、签名及运行环境探测 |
| 89ba615 | [Linux](https://github.com/xzcode/remote-ex/actions/runs/45749483693) | 通过：DEB 解包、依赖信息、组件及运行环境检查 |
| 3d98ba8 | [Core](https://github.com/xzcode/remote-ex/actions/runs/45765313210) | 通过：核心与配置/私有 CA 单元及集成测试 |
| 3d98ba8 | [Native media](https://github.com/xzcode/remote-ex/actions/runs/45765313251) | 通过：隔离媒体与网络测试 |
| 3d98ba8 | [Linux](https://github.com/xzcode/remote-ex/actions/runs/45765313158) | 通过：CJK 系统字体 fixture、Xvfb、Portal pipeline、DEB、运行环境及依赖清单 |
| 3d98ba8 | [Container](https://github.com/xzcode/remote-ex/actions/runs/45765313211) | 通过：非 root、配置预检、隔离健康检查及 SIGTERM |
| 3d98ba8 | [Desktop](https://github.com/xzcode/remote-ex/actions/runs/45765313261) | 失败：默认工作目录 apps/desktop，未找到字典脚本；本候选修复调用目录，脚本本身从 __file__ 定位仓库根 |
| 3d98ba8 | [Windows/macOS native](https://github.com/xzcode/remote-ex/actions/runs/45765313195) | 记录时未结束，不计通过 |

静态检查已核对八套字典的全部 423 个键、无重复或空字符串；不等同母语审校或真实窗口测试。不得用旧提交的成功记录为新发布候选背书。

## 二、alpha.2 发布门槛

所有下列工作流必须针对更改 .github/preview-release.json 的同一个 master 提交成功：Core、Desktop、Native input、Native media、Windows/macOS development builds、Linux X11 development candidate、Signaling container preflight。

各原生构建在完成实际打包和现有 smoke checks 后生成 PACKAGE_EVIDENCE.json。发布任务复核：
- 三个平台各两种格式，共六个文件；核对源提交、发布标签、架构、原生构建 run ID、文件字节数及 SHA-256。
- 便携包内 BUILD_INFO、固定媒体组件清单与实际内容、依赖许可证声明证据的一致性；Linux DEB 架构。
- RELEASE_BUILD.json 汇总七个成功 run ID、精确源码提交和六个文件 SHA-256；SHA256SUMS.txt 覆盖全部六包及 RELEASE_BUILD.json。
- 仅在当前 master 未被替换、没有同名发布、既有 tag 未指向其他提交时创建 GitHub prerelease。发布页必须实际出现八个资产才算完成。
- 不修改 alpha.1；不把自动检查通过等同实际设备验收、签名或法律审查。

## 三、实际设备验收清单（需由授权设备执行）

| ID | 环境 | 必须记录的操作及证据 | 现状 |
|---|---|---|---|
| A01 | Windows x64 | 当前用户 Setup 安装、正常启动、缺少运行库的诊断、完整组件检查；先关闭自启/会话后卸载 | 待实机 |
| A02 | macOS ARM64 | DMG 拖入 Applications、遵循 Gatekeeper、普通 TCC 授权与拒绝、系统字体及真实窗口；关闭自启后移除 | 待实机 |
| A03 | Linux x64 | 兼容发行版 DEB 安装、菜单启动、依赖满足、X11 与 Wayland 分别验证、卸载 | 待实机 |
| A04 | 三平台 | 八语言界面、系统语言变化、字体 fallback、三主题、窄窗口与 DPI，授权文本语义审校 | 待实机/审校 |
| A05 | 两台授权电脑 | LAN 加密测试、单屏只读、键鼠、断开后释放输入、实际显示尺寸与鼠标坐标 | 待实机 |
| A06 | 两个独立网络 | 自建 WSS/直接与强制 TURN、证书错误/ID 错误拒绝、实际候选类型、会话撤销 | 待实机 |
| A07 | 两台授权电脑 | 多屏、音频、剪贴板、指定目录文件传输、受审阅变更、授权范围 | 待实机 |
| A08 | 本地受控环境 | 锁屏/休眠、Portal 撤销、限时可见服务停止、正常退出；不要求绕过系统安全桌面 | 待实机 |
| A09 | 安装与升级 | 发布元数据筛选、文件改动后的 SHA-256 拒绝、手动升级保留用户数据；签名与回退另审 | 候选，待实机 |
| A10 | 自建服务 | 配置预检、独立证书/allowlist/TURN、非 root 运行、重启及撤销流程、实际运维审查 | 容器 fixture 通过，部署待验 |

验收记录仅写源码提交、OS/架构/版本、功能结果、用时与可复现错误。不要在仓库提交真实私钥、证书私钥、设备访问凭据、截图、文件或个人连接记录。无设备不能填写 PASS。

## 四、尚不能标记稳定版完成

项目许可证选择、实际第三方分发义务、GStreamer/外部运行库集成、正式 OS 签名、公证及带可信发布签名的自动更新尚未完成。Windows/macOS 虚拟显示驱动和物理屏幕隐私模式未实现；真实 Wayland 兼容性、远程音频、混合 DPI 与各平台实际安装仍待确认。保留当前短时会话、用户可见、本机授权和停止入口。M6 规划可并行，但不改变这些验收缺口。
