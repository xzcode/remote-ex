#!/usr/bin/env bash
set -euo pipefail
version="${GST_VERSION:?missing GST_VERSION}"
[[ "$version" == "1.26.11" ]] || exit 2
base="https://gstreamer.freedesktop.org/data/pkg/macos/$version"
cd "${RUNNER_TEMP:?}"
for file in "gstreamer-1.0-$version-universal.pkg" "gstreamer-1.0-devel-$version-universal.pkg"; do
  curl --fail --location --retry 3 --output "$file" "$base/$file"
  curl --fail --location --retry 3 --output "$file.sha256sum" "$base/$file.sha256sum"
  shasum -a 256 -c "$file.sha256sum"
  sudo installer -pkg "$file" -target /
done
root=/Library/Frameworks/GStreamer.framework/Versions/1.0
test -x "$root/bin/pkg-config"
echo "$root/bin" >> "$GITHUB_PATH"
echo "PKG_CONFIG=$root/bin/pkg-config" >> "$GITHUB_ENV"
echo "PKG_CONFIG_PATH=$root/lib/pkgconfig" >> "$GITHUB_ENV"
