# M6 移动端运行时验收

基线 8a1263f。历史证据（不混同当前提交）：
- cb882bd Web CI [45779206134](https://github.com/xzcode/remote-ex/actions/runs/45779206134) 完整通过，含独立原生 GStreamer、真实 Xvfb 屏幕、强制 TURN、触屏和浏览器输入测试。
- 8a1263f Core [45781258716](https://github.com/xzcode/remote-ex/actions/runs/45781258716) 和移动端 [45781258591](https://github.com/xzcode/remote-ex/actions/runs/45781258591) 完整通过。Android debug APK 与 unsigned iOS simulator 构建成功；不是物理设备安装。
- 8a1263f 原生媒体 CI [45781258626](https://github.com/xzcode/remote-ex/actions/runs/45781258626) 已通过独立 NAT、forced TURN、实际 decoder 画质切换；Web 完整 profile 2/扩展会话 CI [45781258710](https://github.com/xzcode/remote-ex/actions/runs/45781258710) 尚在运行时，不宣称通过。

## 本批
Android test APK（独立 instrumentation runner，不进入生产 APK）在一次性 GitHub hosted API 36 Google APIs 模拟器安装现有离线应用、启动并检查实际 WebView 安全 origin、6 项 API 探测、身份创建、不可覆盖、后台销毁 WebView、重新打开后同一身份、显式删除。缺 API、KVM 或构建环境则明确失败，不改生产安全限制。iOS 模拟器实际安装、启动、进程存活、终止和重新启动；不是 WKWebView 远端会话验证。测试只使用一次性模拟器，不涉及真实设备、生产身份或外网服务。测试在 artifact 上传后运行，构建与运行时证据独立。两端新运行时 CI 尚未完成。

## 仍需完成的实体设备验收
1. Android/iPhone 实际设备：已批准 HTTPS/WSS、独立身份登记、离开前台后释放，再手动开启。
2. Windows/macOS/Linux 与 Web/手机实际桌面查看；OS 权限、锁屏和撤销检查。
3. 独立物理网络直连/强制中继、5 分钟会话、画质变化释放及恢复输入。
4. 真实键鼠/触摸 OS 端注入、明确撤销、网络断开及性能记录。
5. iOS 签名、系统密钥方案、正式分发与配置审查。保留当前 alpha.2；未发布或部署。

以上验证均不能以源码、CI 排队、构建或纯模拟器测试替代实际物理设备证据。

## Runtime compatibility repair follow-up

8a1263f Web CI 45781258710 **fully passed** including actual profile-2 browser events with native inert Dispatcher, verified held-key/button release, forced TURN Xvfb capture and extended real decoder switching. Core, Linux X11, Native Media and mobile builds at that commit passed. 9f17026 Android runtime CI 45782232173 installed and launched both APKs but failed its original generic six-capability gate. The original log did not identify which capability was missing; no proven underlying WebView cause yet. New diagnostic checks actual Ed25519 sign/verify and export refusal, bounded same-origin WASM binary/MIME/validation and non-streaming initialization, transactional temporary CryptoKey clone/sign proof, recvonly VP8/data-channel SDP (no setLocalDescription/ICE) and pointer API. Android instrumentation now emits sanitized per-capability reason codes and provider version; no real identity/private key printed. Added actual Chromium mobile capability gate and missing-WebRTC branch, pure WASM loader tests. Missing functionality continues to block connection; no weak crypto or TLS fallback. New CI pending. Physical Android/iOS, OS input and cross-network acceptance remain open.
