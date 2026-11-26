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
        if [[ -n "$log" && -f "$log" ]]; then
            grep -Ei 'error|fatal|panic|failed|startup|accelerat|warning' "$log" | tail -n 30 || true
            tail -n 30 "$log" || true
        fi
        bounded 10s adb devices || true
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

mark provision-storage
# Only the newly created disposable AVD; no changes to a physical phone,
# installed application data or storage-security policy.
if [[ "$expectation" = supported ]]; then
    data_mb=6144
    host_available="$(df -Pk "$avd" | awk 'NR==2 {print $4}')"
    [[ "$host_available" =~ ^[0-9]+$ ]]
    printf 'REMOTE EX ANDROID HOST STORAGE: %s KiB available\n' "$host_available"
    if ((host_available< (data_mb+1024)*1024)); then
        echo "Insufficient runner disk for reviewed modern data partition"
        exit 1
    fi
    python3 - "$avd/remote-ex-offline.avd/config.ini" "$data_mb" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
assert p.is_file() and not p.is_symlink()
size = int(sys.argv[2])
assert size == 6144
lines = p.read_text().splitlines()
lines = [line for line in lines if not line.strip().startswith("disk.dataPartition.size=")]
lines.append(f"disk.dataPartition.size={size}M")
p.write_text("\n".join(lines) + "\n")
PY
    printf 'REMOTE EX ANDROID DATA PARTITION: %s MiB\n' "$data_mb"
fi

mark emulator-start
# Modern images (especially the 16 KB page-size variant) can take longer
# than the legacy image's 120-second ADB deadline. Allocate a bounded
# resource budget and distinguish ADB availability from Android boot.
if [[ "$expectation" = supported ]]; then
    gpu=swiftshader
    boot_budget=420
    emulator_resources=(-memory 4096 -cores 4 -partition-size "$data_mb")
else
    gpu=swiftshader_indirect
    boot_budget=270
    emulator_resources=()
fi
printf 'REMOTE EX ANDROID BOOT: gpu=%s budget=%ss\n' "$gpu" "$boot_budget"
bounded 15s "$ANDROID_HOME/emulator/emulator" -accel-check
bounded 15s adb start-server
started=$SECONDS
end=$((started+boot_budget))
"$ANDROID_HOME/emulator/emulator" -avd remote-ex-offline -port 5554 -no-window -no-audio -no-boot-anim -no-snapshot -gpu "$gpu" -accel on -camera-back none -camera-front none -no-metrics "${emulator_resources[@]}" >"$log" 2>&1 &
emulator_pid=$!
mark adb-ready
ready=0
next_report=0
while ((SECONDS<end)); do
    kill -0 "$emulator_pid" || { echo "Emulator exited before ADB became ready"; exit 1; }
    state="$(adb_run 5s get-state 2>/dev/null | tr -d '\r' || true)"
    if [[ "$state" = device ]]; then ready=1; break; fi
    elapsed=$((SECONDS-started))
    if ((elapsed>=next_report)); then
        printf 'REMOTE EX ANDROID WAIT: adb after %ss\n' "$elapsed"
        next_report=$((elapsed+30))
    fi
    sleep 3
done
if [[ "$ready" != 1 ]]; then echo "ADB readiness deadline exceeded"; exit 124; fi
test "$(adb_run 10s shell getprop ro.kernel.qemu | tr -d '\r')" = 1
# Only the serial that did not exist before our emulator was spawned.
owned_serial=1
printf 'REMOTE EX ANDROID ADB READY: %ss\n' "$((SECONDS-started))"
mark boot-complete
ready=0
next_report=0
while ((SECONDS<end)); do
    kill -0 "$emulator_pid" || exit 1
    boot="$(adb_run 8s shell getprop sys.boot_completed 2>/dev/null | tr -d '\r' || true)"
    if [[ "$boot" = 1 ]]; then ready=1; break; fi
    elapsed=$((SECONDS-started))
    if ((elapsed>=next_report)); then
        printf 'REMOTE EX ANDROID WAIT: system boot after %ss\n' "$elapsed"
        next_report=$((elapsed+30))
    fi
    sleep 3
done
if [[ "$ready" != 1 ]]; then echo "Android system boot deadline exceeded"; exit 124; fi
printf 'REMOTE EX ANDROID BOOT READY: %ss\n' "$((SECONDS-started))"
mark prepare-device
adb_run 15s shell getprop ro.build.version.sdk
adb_run 15s shell getprop ro.build.version.release
adb_run 12s shell input keyevent KEYCODE_WAKEUP
adb_run 12s shell wm dismiss-keyguard
adb_run 12s shell settings put global window_animation_scale 0
adb_run 12s shell settings put global transition_animation_scale 0
adb_run 12s shell settings put global animator_duration_scale 0
adb_run 15s shell dumpsys webviewupdate | grep -E "Current WebView package|Preferred WebView package|Minimum WebView version" || true
# Boot completed is not sufficient for PackageInstaller: require the
# package service and actual writable internal storage to be ready.
mark package-storage-ready
apk_bytes=$(( $(stat -c %s "$app") + $(stat -c %s "$testapk") ))
needed_kib=$(( (apk_bytes*8+1023)/1024 + 512*1024 ))
storage_end=$((SECONDS+60))
storage_ok=0
while ((SECONDS<storage_end)); do
    storage="$(adb_run 10s shell df -k /data 2>/dev/null | tr -d '\r' || true)"
    free_kib="$(printf '%s\n' "$storage" | awk '$NF=="/data" && $4 ~ /^[0-9]+$/ {print $4}')"
    if [[ "$free_kib" =~ ^[0-9]+$ ]]; then
        printf 'REMOTE EX ANDROID DATA AVAILABLE: %s KiB (required %s)\n' "$free_kib" "$needed_kib"
        if ((free_kib>=needed_kib)) && adb_run 8s shell pm path android 2>/dev/null | grep -q '^package:'; then
            storage_ok=1
            break
        fi
    fi
    sleep 3
done
if [[ "$storage_ok" != 1 ]]; then
    adb_run 10s shell df -k /data || true
    echo "Package service or internal storage is not ready"
    exit 1
fi
mark install-client
adb_run 60s install "$app"
mark install-test
adb_run 60s install "$testapk"
adb_run 10s shell df -k /data
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
