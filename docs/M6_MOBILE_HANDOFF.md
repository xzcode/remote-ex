# M6 第八批：移动触屏候选

基线 087d2c8；48733f2 Core、Native Media、Linux X11 已通过；Web native/media/capture 最新 CI 仍在运行/排队。

新增 Web 单指显式点击、拖动、右键及滚动模式；复用现有授权、几何及 Protobuf，取消/多指/失焦释放，实际手机待验收。新增移动兼容检测页和窄屏控件适配，连接仍需独立管理员配置、设备身份及 host 授权。没有移动端被控功能、后台会话或真实移动端验收。

Android 打包、静态 HTTPS staging 和 iOS Safari/WKWebView 兼容性仍需独立实现及验证。alpha.2 Release 保持不变。
