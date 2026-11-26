#!/usr/bin/env bash
set -euo pipefail
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(uname -m)" = arm64
test -d "stage/Remote EX.app"
temp="$(mktemp -d "$RUNNER_TEMP/remote-ex-dmg.XXXXXX")"
mount="$temp/mount"
attached=0
cleanup() {
  if [[ "$attached" = 1 ]]; then hdiutil detach "$mount" >/dev/null || return; fi
  rm -rf -- "$temp"
}
trap cleanup EXIT
mkdir -p "$temp/image" "$mount"
ditto stage "$temp/image"
ln -s /Applications "$temp/image/Applications"
out="$PWD/artifacts/Remote-EX-dev-macOS-arm64.dmg"
hdiutil create -volname "Remote EX Preview" -srcfolder "$temp/image" -format UDZO -ov "$out"
hdiutil verify "$out"
hdiutil attach "$out" -readonly -nobrowse -mountpoint "$mount"
attached=1
codesign --verify --deep --strict "$mount/Remote EX.app"
python3 scripts/ci/check_bundle.py verify --dir "$mount/Remote EX.app/Contents/MacOS" --platform macos
python3 scripts/ci/check_bundle.py probe --dir "$mount/Remote EX.app/Contents/MacOS" --platform macos
hdiutil detach "$mount"
attached=0
echo "PASS read-only DMG mounted app and component smoke check"
