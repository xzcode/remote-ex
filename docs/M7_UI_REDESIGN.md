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
