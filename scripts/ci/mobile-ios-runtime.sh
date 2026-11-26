#!/usr/bin/env bash
# Actual offline install/launch, not an iPhone or WKWebView remote session test.
set -euo pipefail
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(uname -s)" = Darwin
app="build/ios-derived/Build/Products/Debug-iphonesimulator/RemoteEX.app"
test -f "$app/RemoteEX"
bundle=com.remoteex.mobile
phase=list-devices
mark() { phase="$1"; printf 'REMOTE EX IOS PHASE: %s\n' "$phase"; }
# macOS does not ship GNU timeout. Python bounds each simctl child and
# reaps it on timeout. No boot/cleanup operation may hang indefinitely.
simctl() {
    local seconds="$1"; shift
    python3 - "$seconds" "$@" <<'PY'
import subprocess,sys
try:
    r=subprocess.run(["xcrun","simctl",*sys.argv[2:]],timeout=int(sys.argv[1]))
    sys.exit(r.returncode)
except subprocess.TimeoutExpired:
    print("simctl phase deadline exceeded",file=sys.stderr)
    sys.exit(124)
PY
}
mark list-devices
device="$(simctl 45 list devices available -j | python3 -c '
import json,sys
d=json.load(sys.stdin)["devices"]
def version(k):
    return tuple(int(x) for x in k.rsplit("iOS-",1)[-1].split("-") if x.isdigit())
for runtime in sorted((k for k in d if "iOS-" in k),key=version,reverse=True):
    for v in d[runtime]:
        if v.get("isAvailable") and v.get("name","").startswith("iPhone") and v.get("state")=="Shutdown":
            print(v["udid"]);sys.exit(0)
raise SystemExit("No available shutdown iPhone simulator")
')"
test -n "$device"
cleanup() {
    local rc=$?
    trap - EXIT INT TERM
    set +e
    if [[ "$rc" != 0 ]]; then printf 'FAIL REMOTE EX IOS PHASE: %s (exit %s)\n' "$phase" "$rc"; fi
    simctl 15 terminate "$device" "$bundle" >/dev/null 2>&1 || true
    simctl 20 uninstall "$device" "$bundle" >/dev/null 2>&1 || true
    simctl 45 shutdown "$device" >/dev/null 2>&1 || true
    exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mark boot
simctl 90 boot "$device"
mark bootstatus
simctl 600 bootstatus "$device" -b
mark install
simctl 90 install "$device" "$app"
launch_check() {
    local result pid
    result="$(simctl 45 launch "$device" "$bundle")"
    pid="$(printf '%s\n' "$result" | awk -v app="$bundle:" '$1 == app && $2 ~ /^[0-9]+$/ {print $2}')"
    test -n "$pid"
    for _ in 1 2 3; do
        sleep 1
        kill -0 "$pid"
        ps -p "$pid" -o comm= | grep -Fq 'RemoteEX'
    done
}
mark first-launch
launch_check
mark terminate
simctl 30 terminate "$device" "$bundle"
mark relaunch
launch_check
mark verified
printf '%s\n' 'PASS REMOTE EX IOS OFFLINE RUNTIME: installed simulator app, process alive, terminated and relaunched (launch smoke only)'
