# M6 第八批：移动触屏候选

基线 087d2c8；48733f2 Core、Native Media、Linux X11 已通过；Web native/media/capture 最新 CI 仍在运行/排队。

新增 Web 单指显式点击、拖动、右键及滚动模式；复用现有授权、几何及 Protobuf，取消/多指/失焦释放，实际手机待验收。新增移动兼容检测页和窄屏控件适配，连接仍需独立管理员配置、设备身份及 host 授权。没有移动端被控功能、后台会话或真实移动端验收。

Android 打包、静态 HTTPS staging 和 iOS Safari/WKWebView 兼容性仍需独立实现及验证。alpha.2 Release 保持不变。

## Android and shared mobile staging candidate

cb882bd 之后追加固定资产 staging、离线默认配置、精确 CSP、禁止 fixture/私钥/临时凭据打包；新增配置验证脚本。Android Gradle 应用壳（AGP 8.7.3 / Gradle 8.9 / JDK17 / SDK35）仅加载随 APK 打包的安全 origin 资源；禁止外部导航、JS bridge、文件/内容访问、混合内容、证书错误绕过和后台服务；后台销毁 WebView，恢复不自动重连。仅 INTERNET 权限，身份采用 origin-local IndexedDB，并非硬件 Keystore。独立 CI 构建离线 debug APK、核对资产。iOS 仅 HTTPS 浏览器候选，未实现原生 iOS App。新 CI、真实手机媒体及生命周期均待验证。详见 apps/mobile/README.md。
