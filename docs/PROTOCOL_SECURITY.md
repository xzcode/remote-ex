# 协议与安全设计 v0.1

状态：设计约束，不是已实现/已审计的协议。

## 威胁模型
不可信互联网、恶意信令/中继、ID 枚举、口令猜测、会话重放、身份替换、越权、恶意文件名及媒体/消息资源耗尽。受控端本机用户应始终知道连接并拥有撤销权限。已被恶意软件完全控制的本机不在可保证安全的范围内。

## 身份与配对
每个安装生成独立设备签名密钥（候选 Ed25519，使用审查过的库），设备 ID 仅作路由标识。注册通过服务端随机挑战及签名证明持有密钥，不接受自行声明在线身份。首次连接默认由本机用户确认，双方通过可信途径比较由双方公钥与会话握手上下文绑定的短认证码；后续连接校验固定公钥。密钥变更要求重新验证，不自动接受。可选临时密码及无人值守固定密码需要经过成熟 PAKE 实现与安全审查；不得直接传输、记录密码或设计可离线猜测的自制握手。未完成前只开放明确确认配对与已授权密钥认证。

## 信令与会话
信令版本字段、请求 ID、session ID、双方 ID、挑战随机数、超时、能力范围；限长、限速、检查发件人身份、收件人和会话归属。注册 -> 在线 -> 请求 -> 用户批准并确认身份 -> 签名绑定媒体 DTLS 指纹、双方身份、角色、随机数、授权与会话标识 -> 协商 -> connected -> closing/closed。SDP/ICE 消息只在归属的活跃会话内转发；早期 ICE 缓存有界。握手消息规范化编码须有确定性，避免歧义；协议字段及签名 transcript 在实现 ADR 评审后锁定。DTLS/SRTP 保障传输机密性；不能用仅有 WSS 或设备 ID 替代对端验证。协商或指纹校验失败，禁止开始采集与输入。

## 逻辑消息
| 类别 | 消息候选 |
|---|---|
| 信令 | register_challenge, register_proof, presence, connect_request, connect_decision, offer, answer, ice_candidate, disconnect |
| 会话控制 | capability_offer, capability_grant, capability_revoke, display_list, select_display, quality_request, ping, close |
| 输入 | pointer_move, pointer_button, wheel, key_down, key_up, release_all |
| 扩展 | clipboard_offer, clipboard_data, file_offer, file_accept, file_chunk, file_cancel, file_complete |
| 错误 | version_mismatch, permission_denied, timeout, resource_limit, unsupported |

控制与授权消息可靠有序；鼠标移动可以使用单独低延迟通道，按键与按钮可靠有序；对过期移动事件丢弃。媒体使用 WebRTC 媒体轨；文件传输独立通道并节流，避免输入拥塞。所有消息验证版本、边界、会话状态、权限和速率。协议 schema 的具体字段编号在 M1/M2 实现时版本化，不以本表充当现成 wire 协议。

## 权限模型
view / input / clipboard-read / clipboard-write / file-send / file-receive / audio / power 分离；默认 view 与 input 也必须明确同意。被控端是最终裁决者，未知权限拒绝。撤销立即生效；会话断开、超时或应用退出时释放所有远程按下的键/按钮，停止采集和输入。无人值守默认关闭，仅用户主动绑定的设备获得指定能力，提供立即撤销与关闭入口。无自动提升权限、无绕过 OS 授权。

## 文件、日志和服务
文件接收根目录由本地用户选择；拒绝绝对路径、路径遍历、危险链接及越界写入；临时文件、大小配额、冲突询问、哈希校验后原子落盘。日志只记录时间、对端 ID、能力与状态，不记录密码、会话密钥、媒体或剪贴板正文。TURN 临时凭据短有效期，限制配额及不允许转发到私有管理网络。密钥与敏感配置不得上传仓库。服务以最小权限运行，IPC 校验本地调用者；托盘/会话提示常驻可见。安装包和更新必须校验发布签名。

## 必须验证
拒绝/撤销、重放、篡改对端指纹、密钥变化、猜测限流、越权、会话隔离、断开清理、路径逃逸及过大消息。未经验证的安全功能标明不可用于生产。 

## M1 第三批实现增量

crates/security/src/session.rs 实现单次 offer/answer 的签名 transcript 原语（固定宽度域隔离编码，签名角色、双方预期 Ed25519 公钥、session ID、双方 nonce、请求/授予权限、原始 SDP 字节的 SHA-256）。两端分别签名，验证者必须从独立可信渠道获得预期公钥并比对实际 SDP 原字节。签名绑定 SDP 内容，进而覆盖其中指纹，但不独立证明 SDP 有效或 DTLS 已验证。此原语尚未与媒体及会话状态机集成。首次配对、随机数生成及一次性消费、持久信任、签名交换的具体 wire schema、DTLS 实际绑定、用户确认、超时和授权仍是必须完成的工作。不能仅凭签名通过开放真实画面或系统输入；签名交换之前若需协商，使用合成/空白视频且阻断输入。当前不是生产级认证协议。

## M1 第四批增量

pairing.rs 增加单次 PendingPairing → ApprovedPairing → SignedPairing → VerifiedSession 状态。每个 attempt 由 OsRng 生成新 nonce，120 秒过期，批准、签名、完成消耗前一状态；固定预期 peer 公钥（必须通过独立可信全指纹校验或既有可信记录获得），拒绝扩大权限，双端签署相同 session/nonce/SDP/grant。core SessionGate::on_verified_transport 要求匹配 session ID、请求及授予权限的 VerifiedSession。此机制尚无 UI、网络信令、信任库或实际 DTLS channel binding；会话 owner 必须确认真实 DTLS 对端与已批准公钥/SDP 一致，并禁止重复 session、控制未授权输入、关闭时释放资源。底层签名验证函数不代替一次性状态机。

## M1 第五批 LAN 信令诊断

crates/signaling 定义 4 字节 big-endian 长度、版本 1、256 KiB frame、64 KiB SDP、候选数量与长度限制、会话标识、严格类型、超时。请求/接受/offer/answer/proof/candidates/finished 顺序由诊断 owner 执行。显式 private/loopback 绑定不等于信任；未加密 TCP 不发送秘密或生产会话数据。实际媒体采用 WebRTC 自有 DTLS/SRTP/SCTP。先比较独立获知的完整设备公钥，再本地确认测试授权；使用双签名证明绑定双方身份、nonce、grant、SDP 原文。仅在该证明通过、两端 WebRTC 连接成功且可靠数据通道 open 后处理固定模拟输入。签名不替代真实 DTLS 身份绑定审计；此版本不允许远程系统输入、真实屏幕、剪贴板、文件或无人值守。未实现持久配对、撤销界面、加密生产信令或长期监听。CI 的父进程在 loopback 显式提供两个受控子进程的临时公钥和测试许可，不是生产自动批准逻辑。

## M1 第六批桌面集成

桌面会话入口仅限 main window、固定媒体子进程、单活动会话、显式具体本地地址和双方完整临时公钥核对、明确同意后发送固定两行本地批准；没有任意命令、任意 sidecar 路径或凭据/远程数据透传。原生监督器不依赖 UI 轮询来执行超时和退出清理，进程输出仅解析已知标记。桌面授权仅适用于合成诊断，不代表真实屏幕或系统输入授权。正式发布前仍需签名并校验媒体组件、保护进程 IPC、解决信令身份与实际 DTLS 绑定评审、持久信任、正式权限及撤销策略。禁止将诊断监听对公网暴露。

## M1 第七批：加密只读会话

独立 secure::SecureWire 使用 Noise KK，双方预先在可信渠道核对完整 X25519+Ed25519 公钥 bundle。Noise transport records 16-bit length，最大 65535 bytes；发送和接收限时，任何错误使通道永久失败。只读 CLI 额外校验双方相同 profile、VIEW-only grant、双方随机数和原始 SDP 的 Ed25519 transcript；在已连接的实际视频 transceiver sender/receiver DTLS transport 中读取 remote-certificate，对 PEM leaf DER 做 SHA-256，与所有已签名 SDP fingerprint 一致校验；缺失或冲突拒绝。若启用数据通道，通用 certificate helper 也检查 SCTP transport，当前只读 CLI 不创建数据通道。必须由两个交互式终端本地批准真实屏幕，只有指定显示器，在加密 Ready 与本地 gate 激活后采集；心跳中断、权限错误或截止停止新帧，进程退出释放 pipeline。无输入/无人值守功能。此实现待实际 GStreamer 证书属性集成测试、安全审查及跨平台验证；严禁把核心 Noise 单测通过当成完整真实屏幕认证已验证。

## M1 第八批：只读桌面与输入边界

UI 区分加密真实共享、加密合成测试和历史明文合成诊断；固定 profile 后禁止更改角色、地址与显示器直至断开。真实共享须有显示器选择、128 hex 双公钥核对、独立明确只读同意；supervisor 管理固定相邻二进制，保留同意管道，媒体子进程在管道消失时退出。无实际输入、无人值守或权限升级。未来正式发行需签名及验证 sidecar，防止本地替换；本阶段依赖可信开发构建。输入 Dispatcher 要求已经验证连接的 SessionGate；所有消息经过 gate，view-only、重复/错序等拒绝，不直接操作 OS。实际输入状态另行跟踪，失败/关闭时释放；原生适配尚未连接到数据通道，不能称为已支持远程键鼠。OS 级释放失败与突然进程终止需要额外设计、实机确认及错误反馈。

## M1 第九批控制 profile

真实控制与只读 profile 隔离：独立程序、双向明确授权 VIEW|INPUT、预先固定双方公钥、Noise、Ed25519 SDP 签名、实际 video 与 SCTP DTLS 证书比对。仅匹配本机明确选择的主显示器；控制窗口失焦或离开内容矩形释放、Esc 停止；host Ctrl-C、心跳丢失/超时、媒体失败触发 Dispatcher::close。通过 STOP/ACK 确认正常释放；强制结束进程或系统注入失败不能保证自动释放，仍需实机与故障恢复设计。默认测试不调用系统输入。真实控制功能限定可见终端，不从 GUI supervisor 自动批准或强杀，禁用无人值守/公网/提权绕过。开发组件尚未正式签名、生产审计与打包。

## M2.1 服务端安全边界

开发服务仅 loopback，无 TLS 或公网绑定；每 socket 新鲜注册 challenge，一次签名尝试，设备 ID 从已验证 Ed25519 key fingerprint 推导，拒绝重复在线 ID。客户端后续必须验证可信服务器 WSS 身份及持久对端公钥；服务器转发的公钥本身不是可信配对证据。连接请求与 host grant 分离，服务端拒绝越权、错误 nonce/角色/会话及 SDP/ICE 顺序、过期或限额；只在路由授权内传递 SDP/签名/ICE，不验证真实 DTLS 或直接启动设备操作。双方独立 nonce 与签名由端点消费一次并核对授权，服务端 grant 不是系统输入许可。生产还需身份注册策略、每来源防滥用、持久信任与撤销、TLS、协商数据保护、真实 DTLS/SCTP 集成及 TURN 安全审查。不得对公网开放本开发原型。

## M2.2 注册传输安全

客户端须先完成 WSS TLS 正常证书链、域名及有效期校验，并核对独立配置的 server ID，再签署 per-socket challenge；server ID 是部署标识而非 TLS 替代或设备权限。无自动明文降级；明确本机开发只允许 numeric IPv4 loopback 与临时身份。GUI 默认临时身份，持久身份需显式勾选系统凭据存储许可；密钥不可读或不合法即失败，不覆写、不退回临时身份。协议对 server frame 也进行字段/长度校验，设备 ID 必须匹配本机公钥指纹。上线预览只发送 Ping，确认 Pong 超时失败，拒绝全部 Incoming，不建立真实远控会话；关闭/退出停止。真正 peer 公钥信任与双方签名/DTLS 验证仍待集成。服务端仍 loopback-only，不部署公网。

## M2.3 路由诊断增量

每次 outgoing 本地 fresh nonce 在 request 之前生成，服务器 session ID 只能一次性绑定，不可覆盖 nonce 或刷新有效期。双方预先独立核对完整设备公钥；服务器路由及 grant 不构成真实授权。WSS 服务端测试使用临时本机证书，真实客户端保留证书链/hostname/server ID 校验。两端签名绑定角色、会话、key、双方 nonce、grant、原始 SDP；验证后交换有界 ICE batch，检查实际 WebRTC 视频与 SCTP DTLS certificate，再直接交换 readiness。固定 synthetic 画面与 inert input，未接入 OS 捕获/输入。现阶段 WSS 保护客户端与服务端链路，但服务端可读取 SDP/ICE，不宣称信令对服务器端到端保密；恶意服务器仍可拒绝/干扰协商。后续需要端到端加密协商与明确安全评审。CLI 不可切换真实模式，桌面 presence 仍拒绝所有请求；服务端仍 loopback-only。

## M2 整合安全边界

- WSS server certificate/hostname + configured deployment ID, device challenge registration, optional explicit operator allowlist; do not expose plaintext prototype. Public bind explicit TLS-only with enrolled device IDs. Server grant alone is routing permission.
- Noise NN ephemeral channel is not authenticated on its own. Sign both peer roles and context + handshake hash with pinned Ed25519 keys and verify peer signature BEFORE SDP/ICE/geometry. No plaintext fallback, no key trust from server. Signed profile/relay choice prevent test→real or view→control substitution. Actual DTLS fingerprint and session transcript verified independently.
- TURN credential issuance once/participant only after approved secure route with both authentication records, TTL180s; shared secret server-only; coturn receives short-lived credentials and must enforce quotas and network egress/peer restrictions. Existing allocations may survive auth expiry. No public anonymous relay. No logging credential content. Signaling server still sees routing metadata.
- Real routed native view/control separate from diagnostic executable, explicit local scope and specific peer pin, normal permissions, 30 seconds, primary display. Stop/heartbeat/stall/transport close triggers release; supervisor EOF cancels and waits for marker. Hard kill/OS errors remain unconfirmed risk. No real automatic reconnect or unattended operation. Native profile and physical cross-network tests required before operational use.

## M3 第二批边界

Only explicitly selected local displays (maximum four) may be viewed in a view-only routed session. The host validates the list before capture, every frame checks display presence/geometry and ordinary OS permission; a fixed view-switch command cannot address arbitrary displays. The composite contains only those authorized screens. Native control stays primary-only. The viewer disarms and releases on focus loss, toolbar, resize, or fullscreen; local menu/toolbar actions are not remote input events. Sender-side first capture precedes READY_ACK. Dedicated per-slot optional persistent identity keys are never exposed; enrollment and trust remain separate from session grant. Local supervised file pause/resume and controller volume commands are bounded, exact LF/CRLF lines; unknown or incomplete lines and EOF revoke. Audio test uses synthetic source and fakesink; no physical microphone/desktop audio tested. No deployment or unattended access.

## M3 第三批内容边界

Checkpoint data is user-selected local partial plaintext, not a capability, token or identity secret. It requires both matching independently verified signing keys, the exact full offer, bounded prefix and source-prefix SHA256 before resume; completion still verifies full SHA256, and no existing path is overwritten. A fresh session and local consent are required even when a checkpoint is available. Do not keep sensitive partial content by default. The directory profile signs kind/direction, selects a local root and a new local destination, validates the complete manifest and length, rejects symlinks/reparse points/unsafe components and uses exclusive creation. No arbitrary remote path commands, deletion or execution. Local concurrent malicious filesystem mutation is outside this development candidate's verified scope; interrupted extraction may leave a partial newly created directory. Clipboard sync has its own signed test/native profile and explicit bidirectional consent, local baseline only, bounded text, no initial transmission, no background access after session, no logging content. Test fixture must never use real clipboard. Keep visible revocation and physical OS checks; no unattended or automatic session grants.

## M3 第四批安全边界

Shared identity does not grant control: controller-only registration is an independently domain-signed fresh challenge with trusted WSS, max 3 sockets, exclusive with normal registration. Each route is owned by the exact authenticated connection, not merely the shared fingerprint. Requests to scoped controller registrations, sibling-session decisions, ICE, signals or closes are refused. Each host still independently verifies the key, transcript, DTLS and local grant. A new read-only catalog profile permits only one locally preselected root's bounded relative manifest and one explicitly selected file index. No remote absolute path, write, deletion, rename, shell, link traversal or overwrite. No automatic file collection: both users approve and the controller selects an explicit entry. Do not persist real manifest content or include it in shared logs. Revocation remains visible. Physical concurrency/file permissions validation pending.

## M3 第五批边界

Audio/video profiles are explicitly separate signed grants, not implicit audio escalation from existing view/control. Native system-output audio capture starts only following independent authorization and verified media peer; no microphone fallback. Receiver default closed audio valve/mute until verified readiness. Check actual DTLS for both RTP transceivers and SCTP; reject profile mismatches. macOS helper uses fixed local pipe and OS permission preflight, limited duration and parent EOF, no arbitrary command, files, or network. Experimental Windows selected-screen input is explicitly opted in, per-monitor DPI and exact physical bounds verified, pointer positions checked and foreground monitor required for key presses; release paths never depend on foreground. Secondary macOS input remains disabled. Neither actual TCC nor physical DPI/A-V synchronization has been accepted; retain ordinary user permission and local revoke. No unattended feature or release/deployment introduced.
