# M7 — Remote EX product UI / UX redesign

2026-11-26. User approved desktop and mobile redesign, using TeamViewer as a workflow reference, not copying its brand, artwork, icons or code.

## Design baseline
- Own Remote EX identity, restrained blue, soft surfaces, consistent 36 px desktop controls, native titlebar, original outline icons; light, soft gray, dark, black and system themes.
- Desktop workspace: one incoming preparation card, one outgoing invitation review card; actual device/connection history/file transfer/server/diagnostic pages. Never show a fake device ID, password, presence or recent connection.
- Invitation parsing fills existing reviewed routed-session fields, not automatic registration or consent. All prior explicit native guards, role/profile choices, host permission, peer-key checks and disconnect remain.
- Trusted devices and history use the real server-scoped local database. Offline presence stays unknown; no automatic network calls. File transfer opens the existing file session mode, not an invented full file explorer.
- Mobile/Web: separate connection landing, saved connections, settings, read-only default, actual session screen and persistent visible disconnect. No unsupported mobile host, clipboard, audio, history or gesture claims.
- Native window title bars remain OS-owned. No new dependency for icons.

## M7 work packages
1. Workspace navigation, invitation review, real device list/history, coherent desktop themes and controls.
2. Mobile landing and responsive viewer; preserve exact asset routes, CSP and capability/identity/consent logic.
3. Native viewer toolbar, session layouts, device selection, file explorer UX and complete localization.
4. Actual preview builds and responsive/keyboard checks, plus independent physical sessions. M6 media/physical gates are still open.

The implementation is independent of the old Linux WebKit black-frame diagnosis and Android emulator runtime acceptance. Do not claim new releases or working physical sessions from interface changes alone.

## Mobile first implementation

Shared mobile/Web assets now implement a Connect / Devices / Settings navigation with saved connection rows, real empty states, scoped search, local identity management and capability details in Settings. No mobile history or phone host navigation until those functions exist. Appearance follows system or an explicit light/gray/dark/black preference; localStorage only stores a theme choice. A saved connection row transfers a bounded single-use reference via sessionStorage to the existing reviewed connection form, verified against the exact configured endpoint and validated local book. Read-only/extended/consent reset and no session starts automatically. The real connection form remains usable, and an authenticated viewing session switches to a screen-first dark workspace with visible Disconnect, actual metrics and existing supported input/quality tools. Layout adapts to landscape without changing input or protocol behavior. Native iOS shell collapses its introductory status and open button while the reviewed controller is displayed, retaining native disconnect. Android serves the one new reviewed UI asset through the same exact allowlist. No new domain, certificate exception, media feature or physical acceptance claim. Mobile shell currently retains the existing English baseline; full mobile translation, virtual keyboard/advanced gestures, native desktop viewer toolbar and file explorer remain subsequent work. Actual browser UI regression uses existing local-only fixture and no sockets or real devices. New CI pending.
