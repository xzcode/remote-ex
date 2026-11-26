#!/usr/bin/env bash
# Fresh hosted runner only. Trust a one-day local CA, remove it on exit.
set -euo pipefail
test "$RUNNER_ENVIRONMENT" = github-hosted
test "$(uname -s)" = Linux
test -n "$GITHUB_RUN_ID"
test -n "$GITHUB_RUN_ATTEMPT"
tmp="$(mktemp -d)"
ca="/usr/local/share/ca-certificates/remote-ex-webkit-ci-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT.crt"
installed=0
cleanup(){
    local rc=$?
    trap - EXIT INT TERM
    set +e
    if [[ "$installed" = 1 ]]; then
        timeout 20s sudo rm -f -- "$ca"
        timeout 45s sudo update-ca-certificates >/dev/null
    fi
    rm -rf -- "$tmp"
    exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
test ! -e "$ca"
umask 077
openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 1 -subj '/CN=Remote EX disposable WebKit CA' \
    -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign,cRLSign' \
    -keyout "$tmp/ca.key" -out "$tmp/ca.pem" 2>/dev/null
openssl req -new -newkey rsa:2048 -nodes -sha256 -subj '/CN=localhost' \
    -keyout "$tmp/server.key" -out "$tmp/server.csr" 2>/dev/null
cat >"$tmp/leaf.ext" <<'EOF'
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost,IP:127.0.0.1
EOF
openssl x509 -req -in "$tmp/server.csr" -CA "$tmp/ca.pem" -CAkey "$tmp/ca.key" -CAcreateserial \
    -days 1 -sha256 -extfile "$tmp/leaf.ext" -out "$tmp/server.pem"
openssl verify -CAfile "$tmp/ca.pem" -verify_hostname localhost "$tmp/server.pem"
openssl verify -CAfile "$tmp/ca.pem" -verify_ip 127.0.0.1 "$tmp/server.pem"
timeout 15s sudo install -m 0644 "$tmp/ca.pem" "$ca"
installed=1
timeout 45s sudo update-ca-certificates
export REMOTE_EX_WEBKIT_CA="$tmp/ca.pem" REMOTE_EX_WEBKIT_CERT="$tmp/server.pem" REMOTE_EX_WEBKIT_KEY="$tmp/server.key"
case "$*" in
    "")
        timeout --kill-after=10s 150s node scripts/ci/test-webkit-controller.mjs
        ;;
    --native-receiver)
        xvfb-run -a -s '-screen 0 1024x768x24 +extension RANDR' env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET XDG_SESSION_TYPE=x11 timeout --kill-after=10s 180s node apps/web/test-view-x11.mjs --webkit
        ;;
    --local-media)
        timeout --kill-after=10s 240s node scripts/ci/test-webkit-controller.mjs --local-media
        ;;
    *) echo "Unsupported WebKit acceptance mode"; exit 1;;
esac
