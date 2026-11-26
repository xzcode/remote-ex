# M6 extended session and runtime quality candidate

Baseline d8a052a: Core and signaling container preflight passed. Mobile iOS job 139773033837 compiled and uploaded unsigned simulator app; Android still building at inspection. Prior f41c864 full native-to-browser and Xvfb screen view passed. Current expanded Web CI remains pending.

- Added explicit operator option --allow-extended and optional mobile config extended:true, still off by default; user selects duration and re-confirms every fresh connection. Rust WASM signs existing profile 22/23; both peers must independently select matching long mode and obtain exact 420-second routing lease before ICE. No automatic extension, reconnect, permission downgrade or unattended grant. Browser foreground view capped at 300s; short 30s unchanged.
- Runtime quality control (Auto/Full/Balanced/Low) uses the existing native D2/D3 data channel protocol, strict status bytes, acknowledgement deadline and actual decoded size checks. Full 960x540 only for keyboard/touch control; quality changes or reduced-resolution frames release input and require explicit reactivation. No scaled-coordinate guess. Added signed scope regression and pure session-policy tests.
- X11 view-only supervised temporary enrollment can explicitly choose extended view; existing host consent remains mandatory. New disposable CI extends actual Xvfb view beyond 30s and switches actual decoder dimensions 480x270 -> 960x540, then verifies close. No real physical device/control acceptance yet.
- Mobile asset packaging includes shared policy module; no default server or release changes. New CI pending. Operator must review actual results before claiming feature ready.

## Connection management and deployment verification follow-up

6c84eeb pure Node session-policy/JS syntax checks passed in a disposable directory on the authorized Seoul host; no live deployment. d8a052a mobile CI 45780050959 both Android debug APK and unsigned iOS Simulator jobs completed successfully, including SDK provisioning, asset inspection and artifact upload. These are offline default builds, not physical-device acceptance or new releases.

Added bounded origin-local saved connection details (24 entries, fixed configured endpoint, exact pins, explicit rename/remove); never stores a grant/secret, loading resets input mode/duration/consent. Added offline staged directory verifier with exact file/header/config validation, WASM header and tree digest; tests reject changed headers, extra files, symlinks. New CI awaits verification. No automatic reconnect or public deployment. Real device/control/relay validation remains open.
