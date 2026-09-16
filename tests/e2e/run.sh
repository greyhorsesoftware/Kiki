#!/usr/bin/env bash
# kiki end-to-end harness (plan 11): starts a headless compositor, kikid and the shell,
# then drives the shell over `qs ipc` and asserts through the daemon socket.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$here/out"; mkdir -p "$out"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-$(mktemp -d)}"
export KIKI_CONFIG_DIR="$(mktemp -d)"
export KIKI_STATE_DIR="$(mktemp -d)"
export KIKI_THUMB_DIR="$(mktemp -d)"
export KIKI_TRASH_DIR="$(mktemp -d)"
export KIKI_HELPER_DIR="${KIKI_HELPER_DIR:-$root/target/release}"
export KIKI_PLUGIN_DIR="${KIKI_PLUGIN_DIR:-$root/target/release}"
export HOME_FIXTURE="$(mktemp -d)"
export KIKI_START="file://$HOME_FIXTURE"
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman

# Fixture home: 10k files, a few folders, an archive, an image.
mkdir -p "$HOME_FIXTURE/Projects/kiki/src" "$HOME_FIXTURE/Pictures" "$HOME_FIXTURE/big"
for i in $(seq 1 10000); do : > "$HOME_FIXTURE/big/file$i.txt"; done
printf 'fn main() {}\n' > "$HOME_FIXTURE/Projects/kiki/src/main.rs"
(cd "$HOME_FIXTURE" && bsdtar -cf archive.tar Projects)

kikid_bin="${KIKID:-$root/target/release/kikid}"
qs_conf="${KIKI_SHELL_DIR:-$root/qml}"

run_in_cage() {
  # cage runs one client; we run a shell that starts kikid, the shell, then the driver.
  cage -- bash -c "
    set -e
    '$kikid_bin' & kikid_pid=\$!
    sleep 0.5
    qs -p '$qs_conf/shell.qml' & qs_pid=\$!
    sleep 2
    python3 '$here/driver.py' '$XDG_RUNTIME_DIR/kiki.sock' '$out'
    rc=\$?
    kill \$qs_pid \$kikid_pid 2>/dev/null || true
    exit \$rc
  "
}

if command -v cage >/dev/null; then
  run_in_cage
else
  echo "cage not installed; running the daemon-only checks" >&2
  "$kikid_bin" & pid=$!
  sleep 0.5
  python3 "$here/driver.py" "$XDG_RUNTIME_DIR/kiki.sock" "$out" --daemon-only
  rc=$?
  kill $pid 2>/dev/null || true
  exit $rc
fi
