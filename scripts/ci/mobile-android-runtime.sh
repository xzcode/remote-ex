#!/usr/bin/env bash
# Only a disposable GitHub-hosted Linux emulator; never a user's phone.
set -euo pipefail
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(uname -s)" = Linux
test -c /dev/kvm
test -n "$ANDROID_HOME"
app="apps/mobile/android/app/build/outputs/apk/debug/app-debug.apk"
testapk="apps/mobile/android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk"
test -f "$app"
test -f "$testapk"
sdkmanager "emulator" "system-images;android-36;google_apis;x86_64"
sudo chown "$(id -u):$(id -g)" /dev/kvm
test -r /dev/kvm && test -w /dev/kvm
export ANDROID_AVD_HOME="$(mktemp -d)"
log="$(mktemp)"
result="$(mktemp)"
serial=emulator-5554
emulator_pid=
cleanup() {
    adb -s "$serial" emu kill >/dev/null 2>&1 || true
    if [[ -n "$emulator_pid" ]]; then kill "$emulator_pid" 2>/dev/null || true; wait "$emulator_pid" 2>/dev/null || true; fi
    rm -rf "$ANDROID_AVD_HOME"
    rm -f "$log" "$result"
}
trap cleanup EXIT
# Refuse to interfere with an existing emulator.
if adb devices | grep -q '^emulator-5554[[:space:]]'; then
    trap - EXIT
    rm -rf "$ANDROID_AVD_HOME"; rm -f "$log" "$result"
    exit 1
fi
printf 'no\n' | avdmanager create avd --name remote-ex-offline --package 'system-images;android-36;google_apis;x86_64' --force
"$ANDROID_HOME/emulator/emulator" -avd remote-ex-offline -port 5554 -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect -accel on -camera-back none -camera-front none -no-metrics >"$log" 2>&1 &
emulator_pid=$!
timeout 180s adb -s "$serial" wait-for-device
ready=0
for _ in $(seq 1 120); do
    if [[ "$(adb -s "$serial" shell getprop sys.boot_completed | tr -d '\r')" = 1 ]]; then ready=1; break; fi
    kill -0 "$emulator_pid" || { tail -n 30 "$log"; exit 1; }
    sleep 2
done
test "$ready" = 1
test "$(adb -s "$serial" shell getprop ro.kernel.qemu | tr -d '\r')" = 1
adb -s "$serial" shell input keyevent KEYCODE_WAKEUP
adb -s "$serial" shell wm dismiss-keyguard
adb -s "$serial" shell settings put global window_animation_scale 0
adb -s "$serial" shell settings put global transition_animation_scale 0
adb -s "$serial" shell settings put global animator_duration_scale 0
adb -s "$serial" shell dumpsys webviewupdate | grep -E "Current WebView package|Preferred WebView package|Minimum WebView version" || true
adb -s "$serial" install "$app"
adb -s "$serial" install "$testapk"
set +e
timeout 220s adb -s "$serial" shell am instrument -w -r com.remoteex.mobile.test/com.remoteex.mobile.OfflineSmoke | tee "$result"
status=$?
set -e
adb -s "$serial" logcat -d -s RemoteEXOfflineSmoke:I "*:S" | tail -n 12 || true
test "$status" = 0
grep -Fq 'PASS REMOTE EX ANDROID OFFLINE RUNTIME:' "$result"
grep -Eq '^INSTRUMENTATION_CODE: -1[[:space:]]*$' "$result"
if grep -Fq 'FAIL REMOTE EX ANDROID OFFLINE RUNTIME:' "$result"; then exit 1; fi
