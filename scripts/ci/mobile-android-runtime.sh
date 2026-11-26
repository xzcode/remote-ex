#!/usr/bin/env bash
# Fresh GitHub-hosted Linux emulator only. Every external phase is bounded.
set -euo pipefail
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(uname -s)" = Linux
test -c /dev/kvm
test -n "$ANDROID_HOME"
api="$REMOTE_EX_ANDROID_API"
expectation="$REMOTE_EX_ANDROID_EXPECTATION"
case "$api:$expectation" in
    36:legacy|modern:supported) ;;
    *) echo "Invalid Android runtime matrix selection"; exit 1 ;;
esac
printf 'REMOTE EX ANDROID MATRIX: %s / %s\n' "$api" "$expectation"
app="apps/mobile/android/app/build/outputs/apk/debug/app-debug.apk"
testapk="apps/mobile/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
test -f "$app"
test -f "$testapk"
phase=setup
serial=emulator-5554
avd=
log=
result=
emulator_pid=
owned_serial=0
mark() { phase="$1"; printf 'REMOTE EX ANDROID PHASE: %s\n' "$phase"; }
bounded() { local seconds="$1"; shift; timeout --kill-after=5s "$seconds" "$@"; }
adb_run() { local seconds="$1"; shift; bounded "$seconds" adb -s "$serial" "$@"; }
cleanup() {
    local rc=$?
    trap - EXIT INT TERM
    set +e
    if [[ "$rc" != 0 ]]; then
        printf 'FAIL REMOTE EX ANDROID PHASE: %s (exit %s)\n' "$phase" "$rc"
        if [[ "$owned_serial" = 1 ]]; then
            adb_run 8s logcat -d -s RemoteEXOfflineSmoke:I '*:S' | tail -n 18 || true
        fi
        if [[ -n "$log" && -f "$log" ]]; then tail -n 25 "$log" || true; fi
    fi
    if [[ -n "$emulator_pid" ]]; then
        if [[ "$owned_serial" = 1 ]]; then adb_run 8s emu kill >/dev/null 2>&1 || true; fi
        kill -TERM "$emulator_pid" 2>/dev/null || true
        for _ in $(seq 1 10); do
            kill -0 "$emulator_pid" 2>/dev/null || break
            sleep 0.5
        done
        kill -KILL "$emulator_pid" 2>/dev/null || true
        # Never wait indefinitely for an unhealthy emulator process.
        if ! kill -0 "$emulator_pid" 2>/dev/null; then wait "$emulator_pid" 2>/dev/null || true; fi
    fi
    if [[ -n "$avd" ]]; then bounded 15s rm -rf -- "$avd" || true; fi
    if [[ -n "$log" && -n "$result" ]]; then rm -f -- "$log" "$result"; fi
    printf 'REMOTE EX ANDROID RUNTIME EXIT: %s\n' "$rc"
    exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mark sdk-catalog
catalog="$(bounded 90s sdkmanager --list --channel=0)"
image="$(printf '%s\n' "$catalog" | python3 scripts/ci/select-android-image.py "$expectation")"
printf 'REMOTE EX ANDROID IMAGE: %s\n' "$image"
unset catalog
mark sdk-install
bounded 300s sdkmanager --channel=0 "emulator" "$image"
bounded 15s sudo chown "$(id -u):$(id -g)" /dev/kvm
test -r /dev/kvm && test -w /dev/kvm
mark avd-create
devices="$(bounded 15s adb devices)"
if grep -q '^emulator-5554[[:space:]]' <<<"$devices"; then
    echo "Existing emulator detected; refusing to interfere."
    exit 1
fi
avd="$(mktemp -d)"
export ANDROID_AVD_HOME="$avd"
log="$(mktemp)"
result="$(mktemp)"
printf 'no\n' | bounded 60s avdmanager create avd --name remote-ex-offline --package "$image" --force

mark emulator-start
"$ANDROID_HOME/emulator/emulator" -avd remote-ex-offline -port 5554 -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect -accel on -camera-back none -camera-front none -no-metrics >"$log" 2>&1 &
emulator_pid=$!
adb_run 120s wait-for-device
test "$(adb_run 10s shell getprop ro.kernel.qemu | tr -d '\r')" = 1
# Only the serial that did not exist before our emulator was spawned.
owned_serial=1
mark boot-complete
ready=0
end=$((SECONDS+150))
while ((SECONDS<end)); do
    kill -0 "$emulator_pid" || exit 1
    boot="$(adb_run 8s shell getprop sys.boot_completed 2>/dev/null | tr -d '\r' || true)"
    if [[ "$boot" = 1 ]]; then ready=1; break; fi
    sleep 2
done
test "$ready" = 1
mark prepare-device
adb_run 12s shell input keyevent KEYCODE_WAKEUP
adb_run 12s shell wm dismiss-keyguard
adb_run 12s shell settings put global window_animation_scale 0
adb_run 12s shell settings put global transition_animation_scale 0
adb_run 12s shell settings put global animator_duration_scale 0
adb_run 15s shell dumpsys webviewupdate | grep -E "Current WebView package|Preferred WebView package|Minimum WebView version" || true
mark install-client
adb_run 60s install "$app"
mark install-test
adb_run 60s install "$testapk"
mark instrumentation
set +e
adb_run 220s shell am instrument -w -r -e expectation "$expectation" com.remoteex.mobile.test/com.remoteex.mobile.OfflineSmoke | tee "$result"
status=$?
set -e
adb_run 10s logcat -d -s RemoteEXOfflineSmoke:I '*:S' | tail -n 18 || true
test "$status" = 0
if [[ "$expectation" = supported ]]; then
    grep -Fq 'PASS REMOTE EX ANDROID OFFLINE RUNTIME:' "$result"
else
    grep -Eq 'PASS REMOTE EX ANDROID (OFFLINE RUNTIME|LEGACY REFUSAL):' "$result"
fi
grep -Eq '^INSTRUMENTATION_CODE: -1[[:space:]]*$' "$result"
if grep -Fq 'FAIL REMOTE EX ANDROID OFFLINE RUNTIME:' "$result"; then exit 1; fi
mark verified
