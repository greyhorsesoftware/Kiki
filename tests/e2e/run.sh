#!/usr/bin/env bash
# kiki end-to-end harness (plans 11 and 28): a headless compositor, a kikid of its own and the
# shell, driven by tests/e2e/driver.py. Everything it touches is a temp directory, so a run
# leaves the machine as it found it.
#
#   tests/e2e/run.sh                 every flow
#   tests/e2e/run.sh --flow trash    one of them
#   KIKI_E2E_KEEP=1 tests/e2e/run.sh keep the fixture home for inspection
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$here/out"; mkdir -p "$out"

work="$(mktemp -d /tmp/kiki-e2e-XXXXXX)"
export XDG_RUNTIME_DIR="$work/run"; mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
export KIKI_CONFIG_DIR="$work/config"; mkdir -p "$KIKI_CONFIG_DIR"
# A fresh config means the first-run dialog ("Make kiki your file manager?") would be up over
# everything, swallowing every key the flows send. A test run must never be asked to change the
# machine's defaults, so the question is answered before the shell starts.
printf '[integration]\nasked = true\n' > "$KIKI_CONFIG_DIR/settings.toml"
export KIKI_STATE_DIR="$work/state"
export KIKI_THUMB_DIR="$work/thumbs"
export KIKI_TRASH_DIR="$work/trash"
export KIKI_PLUGIN_DIR="${KIKI_PLUGIN_DIR:-$root/target/release}"
export HOME_FIXTURE="$work/home"; mkdir -p "$HOME_FIXTURE"
export KIKI_START="file://$HOME_FIXTURE"
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman
# A fake secret-tool: no Secret Service in a headless run, and no flow needs a real keyring.
cat > "$work/secret-tool" <<'EOS'
#!/bin/sh
if [ "$1" = lookup ]; then exit 1; fi
cat >/dev/null; exit 0
EOS
chmod +x "$work/secret-tool"
export KIKI_SECRET_TOOL="$work/secret-tool"

kikid_bin="${KIKID:-$root/target/release/kikid}"
qs_conf="${KIKI_SHELL_DIR:-$root/qml}"
[ -x "$kikid_bin" ] || { echo "no kikid at $kikid_bin — run 'make build' first" >&2; exit 2; }

cleanup() {
  [ -n "${KIKI_E2E_KEEP:-}" ] && { echo "fixture kept at $work"; return; }
  rm -rf "$work"
}
trap cleanup EXIT

run_in_cage() {
  # cage runs one client, so that client is a shell script: daemon, front end, driver. The
  # compositor gets a hard deadline of its own: it has been seen to linger after its client
  # exits, and a hung compositor must not hang a test run.
  timeout -k 5 "${KIKI_E2E_CAGE_TIMEOUT:-600}" cage -- bash -c "
    set -e
    '$kikid_bin' >'$out/kikid.log' 2>&1 & kikid_pid=\$!
    for i in \$(seq 100); do [ -S '$XDG_RUNTIME_DIR/kiki.sock' ] && break; sleep 0.05; done
    qs -p '$qs_conf/shell.qml' >'$out/shell.log' 2>&1 & qs_pid=\$!
    for i in \$(seq 200); do qs -p '$qs_conf/shell.qml' ipc call shell state >/dev/null 2>&1 && break; sleep 0.05; done
    python3 -u '$here/driver.py' '$XDG_RUNTIME_DIR/kiki.sock' '$out' $* 2>&1 | tee '$out/driver.log'
    rc=\${PIPESTATUS[0]}
    kill \$qs_pid \$kikid_pid 2>/dev/null || true
    wait \$qs_pid 2>/dev/null || true
    exit \$rc
  "
}

run_daemon_only() {
  "$kikid_bin" >"$out/kikid.log" 2>&1 & kikid_pid=$!
  for _ in $(seq 100); do [ -S "$XDG_RUNTIME_DIR/kiki.sock" ] && break; sleep 0.05; done
  set +e
  python3 -u "$here/driver.py" "$XDG_RUNTIME_DIR/kiki.sock" "$out" --daemon-only "$@" 2>&1 | tee "$out/driver.log"
  rc=${PIPESTATUS[0]}
  set -e
  kill $kikid_pid 2>/dev/null || true
  return $rc
}

if command -v cage >/dev/null; then
  run_in_cage "$@"
else
  echo "cage is not installed: running the flows that need only the daemon." >&2
  echo "  install it with 'sudo pacman -S cage' to cover the shell flows too." >&2
  run_daemon_only "$@"
fi
