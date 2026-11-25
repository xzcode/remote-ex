export const en = {
  remote: "Remote control", diagnostics: "Local diagnostics", preferences: "Preferences",
  development: "Development build", unavailable: "Not connected", localOnly: "Local diagnostic only",
  remoteHeading: "Connect to a remote device", remoteDescription: "The network session and remote input are not implemented yet. Local display diagnostics are available.",
  openDiagnostics: "Open local diagnostics", incoming: "Incoming connections", incomingOff: "Disabled — no network listener is running.",
  diagTitle: "Screen capture diagnostics", diagDescription: "Select a display and take one local preview. No capture is started automatically, saved to disk or sent over the network.",
  permission: "Screen recording permission", checkPermission: "Check permission", requestPermission: "Request permission",
  permissionUnknown: "Not checked", permissionGranted: "Granted", permissionRequired: "Required — enable screen recording in System Settings, then restart if requested.", permissionSystem: "Controlled by the operating system; enumeration and capture may fail or prompt.",
  refresh: "Refresh displays", display: "Display", select: "Choose a display", primary: "Primary",
  capture: "Capture once", clear: "Clear preview", busy: "Working…", preview: "Local preview",
  previewEmpty: "No image captured", error: "Operation failed", noDisplays: "No available displays in the current desktop session.", browser: "Native desktop runtime is required.",
  language: "Language", appearance: "Appearance", system: "Follow system", light: "Light", dark: "Dark", black: "Pure black",
  saved: "Preferences are stored on this device.", version: "M1 · Local capture preview", noRemote: "Remote control unavailable",
} as const;
export type TextKey = keyof typeof en;
export const zh: Record<TextKey, string> = {
  remote: "远程控制", diagnostics: "本地诊断", preferences: "偏好设置",
  development: "开发版本", unavailable: "未连接", localOnly: "仅本地诊断",
  remoteHeading: "连接远程设备", remoteDescription: "网络会话及远程输入尚未实现，当前可以使用本地屏幕诊断。",
  openDiagnostics: "打开本地诊断", incoming: "接受远程连接", incomingOff: "已关闭：没有运行网络监听服务。",
  diagTitle: "屏幕采集诊断", diagDescription: "选择显示器后手动采集单张预览。不会自动采集、保存到磁盘或通过网络发送。",
  permission: "屏幕录制权限", checkPermission: "检查权限", requestPermission: "申请权限",
  permissionUnknown: "未检查", permissionGranted: "已授权", permissionRequired: "需要授权：请在系统设置中开启屏幕录制权限，必要时重启应用。", permissionSystem: "由操作系统管理；枚举及采集时可能提示授权或失败。",
  refresh: "刷新显示器", display: "显示器", select: "选择显示器", primary: "主屏幕",
  capture: "采集一次", clear: "清除预览", busy: "处理中…", preview: "本地预览",
  previewEmpty: "尚未采集图像", error: "操作失败", noDisplays: "当前桌面会话没有可用显示器。", browser: "需要原生桌面运行环境。",
  language: "语言", appearance: "外观", system: "跟随系统", light: "浅色", dark: "深色", black: "纯黑",
  saved: "偏好设置保存在当前设备。", version: "M1 · 本地采集预览", noRemote: "远程控制尚不可用",
};
