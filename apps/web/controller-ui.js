// Local appearance and one-shot connection-book selection only. Neither
// preference authorizes a session, registers a device or stores secrets.
export const APPEARANCE_KEY="remote-ex.controller.appearance.v1";
export const SELECTION_KEY="remote-ex.controller.selection.v1";
export const THEMES=Object.freeze(["system","light","gray","dark","black"]);
export function applyAppearance(){
    let theme="system";
    try{const v=localStorage.getItem(APPEARANCE_KEY);if(THEMES.includes(v))theme=v;}catch{}
    document.documentElement.dataset.theme=theme;
    return theme;
}
export function setAppearance(theme){
    if(!THEMES.includes(theme))return false;
    document.documentElement.dataset.theme=theme;
    try{localStorage.setItem(APPEARANCE_KEY,theme);return true;}catch{return false;}
}
export function selectForReview(key){
    if(typeof key!=="string"||key.length<1||key.length>2048)return false;
    try{sessionStorage.setItem(SELECTION_KEY,key);return true;}catch{return false;}
}
export function takeSelection(){
    try{const key=sessionStorage.getItem(SELECTION_KEY);sessionStorage.removeItem(SELECTION_KEY);
        return typeof key==="string"&&key.length<=2048?key:"";}catch{return "";}
}

const ZH=Object.freeze({
  "Touch · smart pointer": "触屏 · 智能指针",
  "Slide to move the pointer. Tap for left click, hold briefly and release for right click.": "滑动移动指针，轻点左键，长按后松开触发右键。",
  "Remote EX — Controller": "Remote EX — 远程连接",
  "Remote EX — Browser identity": "Remote EX — 本机身份",
  "Remote EX — Read-only Web controller": "Remote EX — 远程主控",
  "Remote EX home": "Remote EX 首页",
  "Controller": "主控端",
  "Remote connection": "远程连接",
  "Connect to an approved desktop.": "连接已授权的桌面。",
  "Connect to a device": "连接远程设备",
  "No default public server.": "没有默认公共服务器。",
  "Review connection": "检查连接信息",
  "My device identity": "本机身份",
  "Saved connections": "已保存连接",
  "View all": "查看全部",
  "Each session requires verified identities and separate host approval. Your phone cannot accept incoming remote control.": "每次会话均须核对双方身份，并由被控端单独授权。本手机不能接受远程控制。",
  "My devices": "我的设备",
  "Saved connection details on this device, not live presence.": "本机保存的连接信息，不代表实时在线状态。",
  "Search saved devices": "搜索已保存设备",
  "Search devices": "搜索设备",
  "Select a device to review its exact server ID and host key. Saved details never grant access.": "选择设备后核对完整服务器 ID 和主机公钥。保存信息不代表访问授权。",
  "Settings": "设置",
  "Controller preferences and local device information.": "主控端偏好设置及本机信息。",
  "Appearance": "外观",
  "Theme": "主题",
  "System": "跟随系统",
  "Light": "浅色",
  "Soft gray": "柔和灰",
  "Dark": "深色",
  "Black": "纯黑",
  "Language": "语言",
  "Device identity": "设备身份",
  "Checking local identity…": "正在检查本机身份…",
  "Manage identity": "管理身份",
  "Compatibility": "兼容性",
  "Checking this device…": "正在检查本机…",
  "These checks do not verify actual remote video, network or physical input.": "此检查不代表实际远程视频、网络或物理输入已通过验收。",
  "Recheck compatibility": "重新检查兼容性",
  "Controller navigation": "主控端导航",
  "Connect": "连接",
  "Devices": "设备",
  "Theme applies to this page; local preferences unavailable.": "主题仅应用于此页面，本地偏好存储不可用。",
  "Language preference could not be saved.": "无法保存语言偏好。",
  "Local selection unavailable. Open the controller and choose the saved connection there.": "本地选择信息不可用。请打开连接页面后选择已保存连接。",
  "No saved devices.": "没有已保存设备。",
  "Saved connection": "已保存连接",
  "Local identity verified: ": "本机身份已核验：",
  "No local identity. Create one explicitly before connecting.": "尚无本机身份。连接前请主动创建。",
  "Local identity unavailable. Review compatibility or manage identity.": "本机身份不可用，请检查兼容性或管理身份。",
  "Available": "可用",
  "Secure context": "安全上下文",
  "Ed25519 non-extractable key": "Ed25519 不可导出密钥",
  "WASM session core": "WASM 会话核心",
  "IndexedDB": "IndexedDB 本地存储",
  "WebRTC VP8/data channel": "WebRTC VP8 / 数据通道",
  "Pointer events": "指针事件",
  "Required browser APIs detected; actual connection still needs verification.": "已检测到所需浏览器 API，实际连接仍需验证。",
  "Operator endpoint configured. Independent identity verification and host approval required.": "已配置连接地址，仍需独立核对身份并获得主机授权。",
  "No enabled compatible connection configuration. Identity management is local only.": "未启用兼容的连接配置。仅可在本机管理身份。",
  "No connection configuration. Identity management is local only.": "没有连接配置。仅可在本机管理身份。",
  "Compatibility probe failed; no connection attempted.": "兼容性检查失败，未尝试连接。",
  "Ed25519 is required. Update Android System WebView / Chrome to a current supported version (Chromium 137 or newer), or use a compatible current browser. Reopen the app after updating. No alternative identity algorithm or weaker mode is available.": "需要 Ed25519。请将 Android System WebView / Chrome 更新至受支持版本（Chromium 137 或更新），或使用兼容的新版浏览器。更新后重新打开应用。不提供替代身份算法或弱化安全模式。",
  "Update the browser or WebView, reopen the app and recheck. No security downgrade is available.": "请更新浏览器或 WebView，重新打开应用并检查。不提供安全降级。",
  "Back": "返回",
  "Back to controller": "返回主控端",
  "Local identity": "本机身份",
  "Create a device identity for this browser origin. The non-extractable private key stays in browser storage; the public device ID and key can be used for future administrator enrollment. This page cannot connect to a remote desktop or register with a server.": "为当前浏览器来源创建设备身份。不可导出私钥保存在浏览器存储中，公开设备 ID 和公钥用于管理员录入。本页不能连接远程桌面或注册服务器。",
  "Checking storage…": "正在检查存储…",
  "Checking local identity compatibility…": "正在检查本机身份兼容性…",
  "Device ID (SHA-256 public-key fingerprint)": "设备 ID（公钥 SHA-256 指纹）",
  "Public key (Ed25519, hexadecimal)": "公钥（Ed25519，十六进制）",
  "Use the same browser profile and origin, including the same port, to retain this identity. Clearing browser data removes it. This is not server enrollment and grants no viewing or input permissions.": "请使用同一浏览器配置及来源（含相同端口）保留身份。清除浏览器数据会删除身份。本操作不代表服务器录入，也不授予查看或输入权限。",
  "Create identity": "创建身份",
  "Refresh": "刷新",
  "I understand that removing the local key is irreversible and does not revoke server enrollment or active sessions.": "我理解删除本机密钥不可撤销，且不会撤销服务器录入或正在进行的会话。",
  "Remove local identity": "删除本机身份",
  "Back to the synthetic compatibility lab": "返回兼容性实验页",
  "Back to mobile controller": "返回主控端",
  "Local identity ready": "本机身份已就绪",
  "Stored non-extractable identity verified.": "已验证保存的不可导出身份。",
  "No local identity": "尚无本机身份",
  "Local Ed25519 signing and key storage verified. Create a device identity explicitly.": "本地 Ed25519 签名和密钥存储已验证。请主动创建设备身份。",
  "Identity creation unavailable": "无法创建身份",
  "Identity compatibility: ": "身份兼容性：",
  "Checking local identity and storage…": "正在检查本机身份与存储…",
  "Working…": "处理中…",
  "Identity unavailable": "身份不可用",
  "Refresh with a supported browser or WebView. No new identity will be created automatically.": "请使用受支持的浏览器或 WebView 刷新。不会自动创建新身份。",
  "Browser identity could not be verified or stored. Refresh, or explicitly remove the local record to recover.": "无法核验或保存浏览器身份。请刷新，或明确删除本地记录以恢复。",
  "Loading configuration…": "正在加载配置…",
  "Disconnect": "断开连接",
  "Connect to an approved desktop": "连接已授权桌面",
  "Read-only viewing is the default. An explicitly enabled operator policy may offer keyboard and mouse control, which requires separate host approval and local activation. Audio, clipboard and files are not supported.": "默认仅查看。运营方明确启用后可申请键盘和鼠标控制，仍需被控端单独授权并在本机启用输入。不支持声音、剪贴板及文件。",
  "Connection details": "连接信息",
  "Checking local browser capabilities…": "正在检查本机浏览器能力…",
  "Detailed checks": "详细检查",
  "Operator-configured WSS endpoint": "运营方配置的 WSS 地址",
  "Saved connections (local details only)": "已保存连接（仅本地信息）",
  "Select a connection": "选择连接",
  "Connection name": "连接名称",
  "e.g. Office desktop": "例如：办公电脑",
  "Save details": "保存连接",
  "Remove selected": "删除所选连接",
  "Saved details never authorize a session.": "保存连接信息不代表授权。",
  "Server ID (64 lowercase hexadecimal characters)": "服务器 ID（64 位小写十六进制）",
  "Host Ed25519 public key (64 hexadecimal characters)": "主机 Ed25519 公钥（64 位十六进制）",
  "Obtain both values independently from your administrator and host. The page does not discover or trust unknown devices automatically.": "请分别通过可信渠道从管理员与主机取得完整信息。本页不会自动发现或信任未知设备。",
  "Your browser device ID": "本机浏览器设备 ID",
  "Your browser public key": "本机浏览器公钥",
  "Manage browser identity": "管理浏览器身份",
  "Request screen and input permission (experimental).": "申请屏幕及输入权限（实验功能）。",
  "Request up to five minutes (separate host approval).": "申请最多五分钟会话（需主机单独授权）。",
  "I verified the identities and request one read-only session (up to 30 seconds after approval).": "我已核对双方身份，申请一次仅查看会话（批准后最多 30 秒）。",
  "Remote screen": "远程画面",
  "Approved remote screen": "已授权远程画面",
  "Next display": "下一屏",
  "No active screen sharing": "当前未共享画面",
  "Frames": "解码帧",
  "Resolution": "分辨率",
  "Remaining": "剩余时间",
  "Input method": "输入方式",
  "Mouse / physical keyboard": "鼠标 / 实体键盘",
  "Touch · left click": "触屏 · 左键",
  "Touch · drag": "触屏 · 拖动",
  "Touch · right click": "触屏 · 右键",
  "Touch · scroll": "触屏 · 滚动",
  "Enable input": "启用输入",
  "Release input": "释放输入",
  "Input disabled; focus the remote screen only after activation.": "输入已禁用；启用后请保持远程画面焦点。",
  "Stream quality": "画质",
  "Auto": "自动",
  "Full": "高",
  "Balanced": "均衡",
  "Low": "低",
  "Apply": "应用",
  "Transport: —": "连接方式：—",
  "No approved displays": "没有已授权显示器",
  "Uses operator-approved ICE endpoints only; no default server, automatic reconnect or background viewing. Leaving or hiding this page disconnects. The host can revoke sharing independently.": "仅使用运营方批准的 ICE 地址；没有默认服务器、自动重连或后台查看。离开或隐藏本页会断开连接，主机可独立撤销共享。",
  "Saved details never authorize a session. Verify both identities independently.": "保存信息不代表会话授权。请独立核对双方身份。",
  "Local connection storage unavailable or invalid. Manual connections remain available.": "本地连接簿不可用或无效，仍可手动填写连接信息。",
  "Connection details were not changed: ": "连接信息未修改：",
  "Checking configuration and capabilities": "正在检查配置及兼容性",
  "Connection unavailable: ": "连接不可用：",
  "Required browser capabilities verified locally (not a remote session).": "已在本机核验所需浏览器能力（尚未建立远程会话）。",
  "Local compatibility check failed. No connection attempted.": "本机兼容性检查失败，未尝试连接。",
  "Ready. Enter the independently verified identities.": "已就绪，请填写独立核对过的身份信息。",
  "Create a browser identity first.": "请先创建浏览器身份。",
  "Configuration or local identity unavailable. Recheck or manage identity.": "配置或本机身份不可用。请重新检查或管理身份。",
  "I verified the identities and request one {scope} session (up to {duration}). Separate host approval is required.": "我已核对双方身份，申请一次{scope}会话（最多{duration}）。仍需主机单独授权。",
  "screen AND input": "屏幕及输入",
  "read-only": "仅查看",
  "five minutes": "五分钟",
  "30 seconds": "30 秒",
  "Transport: ": "连接方式：",
  "TURN relay": "TURN 中继",
  "Direct": "直连",
  "Verifying": "核验中",
  "All {count} approved displays": "全部 {count} 个已授权显示器",
  "Display {index} of {count}": "显示器 {index} / {count}",
  "Host quality: ": "主机画质：",
  " — applying": " — 正在应用",
  " — input disabled at reduced resolution": " — 低分辨率下输入已禁用",
  "Awaiting host status": "等待主机状态",
  "Input active — keep focus on the remote screen. Leaving it releases input.": "输入已启用，请保持远程画面焦点。离开焦点会释放输入。",
  "Input disabled; click Enable input again to resume.": "输入已禁用，点击“启用输入”后方可恢复。",
  "Verifying local identity": "正在核验本机身份",
  "Waiting for explicit host screen and input approval": "等待主机明确授权屏幕及输入",
  "Waiting for host read-only approval": "等待主机明确授权查看",
  "Waiting for explicit host screen and input approval — up to 5 minutes": "等待主机授权屏幕及输入（最多五分钟）",
  "Waiting for host read-only approval — up to 5 minutes": "等待主机授权查看（最多五分钟）",
  "Authenticating encrypted route": "正在认证加密会话",
  "Negotiating authenticated video transport": "正在协商认证视频传输",
  "Waiting for approved host screen": "等待主机共享授权画面",
  "Connected — input disabled until explicitly enabled": "已连接 — 输入尚未启用",
  "Connected — read only": "已连接 — 仅查看",
  "Disconnecting": "正在断开",
  "Disconnected — host confirmed": "已断开 — 主机已确认",
  "Show tools": "展开工具栏",
  "Hide tools": "收起工具栏",
  "Session tools": "会话工具"
});
export const LANGUAGE_KEY="remote-ex.controller.language.v1";
const SETTINGS_KEY="remote-ex.controller.language-settings.v1";
export const LANGUAGES=Object.freeze(["system","en","zh"]);
let locale="en";
export function resolveLanguage(preference,preferred=[]){
    if(preference==="en"||preference==="zh")return preference;
    for(const value of preferred){const p=String(value).toLowerCase().split(/[-_]/)[0];if(p==="zh"||p==="en")return p;}
    return "en";
}
export function uiText(english,values){
    const value=locale==="zh"&&Object.hasOwn(ZH,english)?ZH[english]:english;
    return values?value.replace(/\{([A-Za-z]+)\}/g,(whole,key)=>Object.hasOwn(values,key)?String(values[key]):whole):value;
}
export function applyLanguage(){
    let preference="system";
    try{const stored=localStorage.getItem(LANGUAGE_KEY);if(LANGUAGES.includes(stored))preference=stored;}catch{}
    locale=resolveLanguage(preference,navigator.languages?.length?navigator.languages:[navigator.language]);
    document.documentElement.lang=locale==="zh"?"zh-CN":"en";
    // Fixed HTML only, before user/device data is rendered. No innerHTML.
    if(locale==="zh"){
        document.title=uiText(document.title);
        const walker=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT);
        for(let node=walker.nextNode();node;node=walker.nextNode()){
            if(node.parentElement?.closest("script,style"))continue;
            const raw=node.nodeValue,trimmed=raw?.trim();
            if(trimmed&&Object.hasOwn(ZH,trimmed))node.nodeValue=raw.replace(trimmed,uiText(trimmed));
        }
        for(const element of document.querySelectorAll("[placeholder],[aria-label]")){
            for(const name of ["placeholder","aria-label"]){const v=element.getAttribute(name);if(v&&Object.hasOwn(ZH,v))element.setAttribute(name,uiText(v));}
        }
    }
    return preference;
}
export function setLanguage(preference){
    if(!LANGUAGES.includes(preference))return false;
    try{localStorage.setItem(LANGUAGE_KEY,preference);}catch{return false;}
    try{sessionStorage.setItem(SETTINGS_KEY,"1");}catch{}
    return true;
}
export function takeSettingsPage(){
    try{const v=sessionStorage.getItem(SETTINGS_KEY);sessionStorage.removeItem(SETTINGS_KEY);return v==="1";}catch{return false;}
}
