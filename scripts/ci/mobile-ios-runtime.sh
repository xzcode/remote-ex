#!/usr/bin/env bash
# Actual offline install/launch, not an iPhone or WKWebView remote session test.
set -euo pipefail
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(uname -s)" = Darwin
app="build/ios-derived/Build/Products/Debug-iphonesimulator/RemoteEX.app"
test -f "$app/RemoteEX"
bundle=com.remoteex.mobile
device="$(xcrun simctl list devices available -j | python3 -c '
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
    xcrun simctl terminate "$device" "$bundle" >/dev/null 2>&1 || true
    xcrun simctl uninstall "$device" "$bundle" >/dev/null 2>&1 || true
    xcrun simctl shutdown "$device" >/dev/null 2>&1 || true
}
trap cleanup EXIT
xcrun simctl boot "$device"
xcrun simctl bootstatus "$device" -b
xcrun simctl install "$device" "$app"
launch_check() {
    local result pid
    result="$(xcrun simctl launch "$device" "$bundle")"
    pid="$(printf '%s\n' "$result" | awk -v app="$bundle:" '$1 == app && $2 ~ /^[0-9]+$/ {print $2}')"
    test -n "$pid"
    for _ in 1 2 3; do
        sleep 1
        kill -0 "$pid"
        ps -p "$pid" -o comm= | grep -Fq 'RemoteEX'
    done
}
launch_check
xcrun simctl terminate "$device" "$bundle"
launch_check
printf '%s\n' 'PASS REMOTE EX IOS OFFLINE RUNTIME: installed simulator app, process alive, terminated and relaunched (launch smoke only)'
