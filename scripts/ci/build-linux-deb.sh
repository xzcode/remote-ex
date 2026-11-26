#!/usr/bin/env bash
set -euo pipefail
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(dpkg --print-architecture)" = amd64
[[ "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]
[[ "$GITHUB_RUN_NUMBER" =~ ^[0-9]+$ ]]
stage="$PWD/dist/linux-dev"
out="$PWD/dist/Remote-EX-dev-Linux-x64.deb"
version="0.1.0~dev.$GITHUB_RUN_NUMBER+${GITHUB_SHA:0:12}"
if [[ -n "${REMOTE_EX_BUILD_TAG:-}" ]]; then
  [[ "$REMOTE_EX_BUILD_TAG" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)-alpha\.([0-9]+)$ ]]
  version="${BASH_REMATCH[1]}~alpha.${BASH_REMATCH[2]}"
fi
temp="$(mktemp -d "$RUNNER_TEMP/remote-ex-deb.XXXXXX")"
trap 'rm -rf -- "$temp"' EXIT
root="$temp/pkg"
mkdir -p "$root/opt/remote-ex" "$root/DEBIAN" "$root/usr/share/applications" "$root/usr/share/pixmaps" "$temp/debian"
cp -a "$stage/." "$root/opt/remote-ex/"
cp apps/desktop/src-tauri/icons/icon.png "$root/usr/share/pixmaps/remote-ex.png"
cat > "$root/usr/share/applications/remote-ex.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Remote EX
Comment=Authorized remote desktop preview
Exec=/opt/remote-ex/remote-ex-desktop
Icon=remote-ex
Terminal=false
Categories=Network;RemoteAccess;
EOF
cat > "$temp/debian/control" <<'EOF'
Source: remote-ex
Section: net
Priority: optional
Maintainer: Remote EX Project

Package: remote-ex
Architecture: amd64
Description: Remote EX experimental remote desktop
EOF
args=()
for bin in remote-ex-desktop routed_x11_view routed_diagnostic lan_view lan_diagnostic remote-ex-rendezvous; do
  args+=("-e$root/opt/remote-ex/$bin")
done
deps="$(cd "$temp" && dpkg-shlibdeps -O "${args[@]}")"
deps="${deps#shlibs:Depends=}"
test -n "$deps"
cat > "$root/DEBIAN/control" <<EOF
Package: remote-ex
Version: $version
Architecture: amd64
Section: net
Priority: optional
Maintainer: Remote EX Project
Depends: $deps, gstreamer1.0-plugins-base, gstreamer1.0-plugins-good, gstreamer1.0-plugins-bad, gstreamer1.0-nice, gstreamer1.0-pipewire
Recommends: xdg-desktop-portal
Description: Remote EX experimental remote desktop
 Authorized local sessions only; no automatic service or public listener.
EOF
dpkg-deb --root-owner-group --build "$root" "$out"
dpkg-deb --info "$out"
dpkg-deb --extract "$out" "$temp/extracted"
python3 scripts/ci/check_bundle.py verify --dir "$temp/extracted/opt/remote-ex" --platform linux
python3 scripts/ci/check_bundle.py probe --dir "$temp/extracted/opt/remote-ex" --platform linux
test "$(dpkg-deb -f "$out" Architecture)" = amd64
echo "PASS DEB extraction and component/runtime checks"
