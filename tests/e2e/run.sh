#!/usr/bin/env bash
# kiki end-to-end harness (plans 11 and 28): a headless compositor, a kikid of its own and the
# shell, driven by tests/e2e/driver.py. Everything it touches is a temp directory, so a run
# leaves the machine as it found it.
#
#   tests/e2e/run.sh                 every flow under cage, then the chooser flows under sway
#   tests/e2e/run.sh --flow trash    one of them
#   tests/e2e/run.sh --part servers  one half: the flows that start a real server (or --part local)
#   tests/e2e/run.sh --part chooser  the flows that need a layer surface, under sway
#   tests/e2e/run.sh --flow gallery_perf   a thousand photographs, timed (not in the default run)
#   KIKI_E2E_KEEP=1 tests/e2e/run.sh keep the fixture home for inspection
#
# Two compositors, because no one of them does both things (docs/0.5.0/11-chooser-window.md):
# `cage` runs the suite as it always has, and `sway` runs the flows that need a layer surface —
# the file chooser, which is above every window or it is behind the application that asked.
# cage has no `wlr-layer-shell` at all, so a chooser shown there is a chooser nobody can see.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$here/out"; mkdir -p "$out"

# The desktop mode (below) hands the daemon and the shell a runtime directory of their own, but
# hyprctl and the screen recorder must keep looking in the real one; the demo flow uses these.
export KIKI_E2E_REAL_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
desktop_display="$KIKI_E2E_REAL_RUNTIME_DIR/${WAYLAND_DISPLAY:-wayland-1}"
# On disk, under target/, not in /tmp: /tmp is a tmpfs of a few GB here, and a run — two 1.5 GB
# upload fixtures, a mirror's worth of copies — filled it once (2026-09-25), which took the
# machine's shells down with it. target/ is ignored by git and cleaned by `cargo clean`.
mkdir -p "$root/target/e2e"
work="$(mktemp -d "$root/target/e2e/run-XXXXXX")"
export XDG_RUNTIME_DIR="$work/run"; mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
# One name for the run's socket, whatever the version: the daemon, the window and the driver
# all read KIKI_SOCKET (by default each is named for its version, `kiki-<version>.sock`).
export KIKI_SOCKET="$XDG_RUNTIME_DIR/kiki.sock"
# The harness starts the daemon and kills it at the end: its life is not its own here, and a gap
# between flows must not take it down (docs/0.3.0/01-daemon-on-demand.md).
export KIKI_EXIT_GRACE_MS=0
# And if it ever does have to be started — a window whose daemon died — it is the one under test,
# never whatever `kikid` is installed on the machine running the suite.
export KIKI_DAEMON="$root/target/release/kikid"
export KIKI_CONFIG_DIR="$work/config"; mkdir -p "$KIKI_CONFIG_DIR"
# A fresh config means the first-run dialog ("Make kiki your file manager?") would be up over
# everything, swallowing every key the flows send. A test run must never be asked to change the
# machine's defaults, so the question is answered before the shell starts.
printf '[integration]\nasked = true\n' > "$KIKI_CONFIG_DIR/settings.toml"
# Project mode starts an editor and an agent, each in a terminal of its own. In a test run those
# are two more windows in a compositor that looks at whichever came last: they took the keyboard
# from kiki for the rest of the run, and whether kiki or a terminal had the focus at any moment
# was a matter of which started faster. The tools are there to be started, not to be used, so
# here they are programs that start, draw nothing and end.
cat > "$KIKI_CONFIG_DIR/open-in.toml" <<'EOT'
[[tool]]
id = "neovim"
detect = ""
command = "true"
reuse = "true"
terminal = false

[[tool]]
id = "claude"
detect = ""
command = "true"
terminal = false
EOT
export KIKI_STATE_DIR="$work/state"
export KIKI_THUMB_DIR="$work/thumbs"
export KIKI_TRASH_DIR="$work/trash"
export HOME_FIXTURE="$work/home"; mkdir -p "$HOME_FIXTURE"
export KIKI_START="file://$HOME_FIXTURE"
export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman
# No application is ever started under the harness: the daemon's `KIKI_OPEN_WITH` runs this in
# place of one, and it only writes what it was given to a log a flow can read (quick_look checks
# the log stays empty — a look starts nothing).
export KIKI_E2E_OPEN_LOG="$work/open.log"
cat > "$work/open-with" <<EOS
#!/bin/sh
printf '%s\n' "\$@" >> "$KIKI_E2E_OPEN_LOG"
EOS
chmod +x "$work/open-with"; export KIKI_OPEN_WITH="$work/open-with"
# A secret-tool of our own: there is no Secret Service in a headless run. It keeps what it is given
# in the temp directory and gives it back, because a flow that signs in to a server with a
# password (remote_transfers, FTPS) needs the daemon to be able to look that password up again.
#   secret-tool store --label L app kiki location <loc> field <f>   (the secret on stdin)
#   secret-tool lookup|clear     app kiki location <loc> field <f>
mkdir -p "$work/secrets"
cat > "$work/secret-tool" <<EOS
#!/bin/sh
verb="\$1"; shift
[ "\$verb" = store ] && shift 2
f="$work/secrets/\$(printf '%s' "\$*" | tr -c 'A-Za-z0-9._-' '_')"
case "\$verb" in
  store) cat > "\$f" ;;
  lookup) [ -f "\$f" ] && cat "\$f" || exit 1 ;;
  clear) rm -f "\$f" ;;
esac
EOS
chmod +x "$work/secret-tool"
export KIKI_SECRET_TOOL="$work/secret-tool"

# Prefer this checkout's build; fall back to an installed kiki, which is what CI tests.
export KIKI_PLUGIN_DIR="${KIKI_PLUGIN_DIR:-$root/target/release}"
kikid_bin="${KIKID:-$root/target/release/kikid}"
if [ ! -x "$kikid_bin" ]; then
  kikid_bin="$(command -v kikid || true)"
  [ -n "$kikid_bin" ] || { echo "no kikid: build with 'make build', or install the package" >&2; exit 2; }
  export KIKI_PLUGIN_DIR="${KIKI_PLUGIN_DIR:-/usr/lib/kiki/plugins}"
  qs_default=/usr/share/kiki
fi
qs_conf="${KIKI_SHELL_DIR:-${qs_default:-$root/qml}}"
[ -f "$qs_conf/shell.qml" ] || { echo "no shell.qml under $qs_conf" >&2; exit 2; }
# The launcher flow runs the real `kiki` script, which finds the shell through this.
export KIKI_SHELL_DIR="$qs_conf"
export KIKI_E2E_OUT="$out"
# The flows compare English words whatever the desktop speaks (docs/0.2.0/02-localization.md).
export LANGUAGE="${KIKI_E2E_LANGUAGE:-en}"       # KIKI_E2E_LANGUAGE=es|ja for the photographs flow

# kiki's SMB locations go through GVfs (plan 25): the plugin asks `gvfsd` on the session bus,
# which runs `gvfsd-smb`, which is what actually speaks to the server. A test run must neither use
# nor disturb the developer's own gvfs — a share it mounted would appear in their file choosers,
# and one it failed to unmount would outlive the run — so the daemon, and only the daemon, is
# given a session bus of its own with a gvfsd of its own on it. The shell keeps the real bus, so
# nothing else about a run changes. Everything started on that bus carries this run's
# XDG_RUNTIME_DIR, which is how the sweep at the end of run_in_cage finds it; its mounts live
# inside gvfsd and go when gvfsd does, and --no-fuse (with GVFS_DISABLE_FUSE for a gvfsd that
# something else activates) keeps it from leaving a mount under $XDG_RUNTIME_DIR/gvfs at all.
export GVFS_DISABLE_FUSE=1
gvfsd_bin=""
for g in /usr/lib/gvfsd /usr/libexec/gvfsd /usr/lib/gvfs/gvfsd; do
  [ -x "$g" ] && { gvfsd_bin="$g"; break; }
done
# Written out rather than quoted inline: it is handed to cage's `bash -c`, which is one layer of
# quoting too many already.
if [ -n "$gvfsd_bin" ] && command -v dbus-run-session >/dev/null; then
  cat > "$work/start-kikid" <<EOS
#!/bin/sh
exec dbus-run-session -- /bin/sh -c '"$gvfsd_bin" --no-fuse >"$out/gvfsd.log" 2>&1 & exec "$kikid_bin" >"$out/kikid.log" 2>&1'
EOS
else
  echo "no dbus-run-session or gvfsd: the daemon shares this session's bus, and SMB flows will skip" >&2
  cat > "$work/start-kikid" <<EOS
#!/bin/sh
exec '$kikid_bin' >'$out/kikid.log' 2>&1
EOS
fi
chmod +x "$work/start-kikid"

# Servers a flow started (tests/e2e/servers.py notes their pids here) and did not live to stop.
export KIKI_E2E_PIDS="$work/server-pids"
cleanup() {
  if [ -f "$KIKI_E2E_PIDS" ]; then
    while read -r pid; do kill "$pid" 2>/dev/null || true; done < "$KIKI_E2E_PIDS"
  fi
  # The same sweep run_in_cage does, once more out here: the private bus and the gvfsd on it are
  # started by the daemon's launcher, and a run without a compositor never reaches that sweep.
  # Nothing outside this run has this runtime directory in its environment.
  for e in /proc/[0-9]*/environ; do
    p=${e#/proc/}; p=${p%/environ}
    { [ "$p" = "$$" ] || [ "$p" = "$PPID" ]; } && continue
    grep -qzx "XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR" "$e" 2>/dev/null && kill "$p" 2>/dev/null || true
  done
  [ -n "${KIKI_E2E_KEEP:-}" ] && { echo "fixture kept at $work"; return; }
  rm -rf "$work"
}
trap cleanup EXIT

# The one client both compositors run: daemon, front end, driver. Written to a file rather than
# quoted into a `bash -c` — sway's config `exec`s it, cage takes it as an argument, and one copy
# of it means the two cannot drift apart. $1 is where the driver's verdict is written, $2 the log
# the run is teed to, and the rest are the driver's own arguments.
write_client() {
  local rc_file="$1" log="$2"; shift 2
  cat > "$work/client" <<EOS
#!/usr/bin/env bash
'$work/start-kikid' & kikid_pid=\$!
for i in \$(seq 100); do [ -S '$XDG_RUNTIME_DIR/kiki.sock' ] && break; sleep 0.05; done
qs -p '$qs_conf/shell.qml' >'$out/shell.log' 2>&1 & qs_pid=\$!
for i in \$(seq 200); do qs -p '$qs_conf/shell.qml' ipc call shell state >/dev/null 2>&1 && break; sleep 0.05; done
python3 -u '$here/driver.py' '$XDG_RUNTIME_DIR/kiki.sock' '$out' $* 2>&1 | tee '$log'
rc=\${PIPESTATUS[0]}
echo \$rc > '$rc_file'
kill \$qs_pid \$kikid_pid 2>/dev/null || true
wait \$qs_pid 2>/dev/null || true
# A compositor stays up for as long as anything is drawing in it, and project mode's terminals
# are started detached and outlive the shell — so a whole run used to sit here until the deadline
# and fail with 124, every check passed. Whatever else was started inside this run is known by
# the runtime directory in its environment, which no process outside the run has. The verdict is
# written above this sweep on purpose: under sway the compositor carries that same runtime
# directory and is swept away with everything else, which is how sway is asked to leave.
for e in /proc/[0-9]*/environ; do
  p=\${e#/proc/}; p=\${p%/environ}
  { [ "\$p" = "\$\$" ] || [ "\$p" = "\$PPID" ]; } && continue
  grep -qzx 'XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR' "\$e" 2>/dev/null && kill "\$p" 2>/dev/null || true
done
exit \$rc
EOS
  chmod +x "$work/client"
}

# Neither compositor hands back its client's exit status: a run with failed checks used to leave
# here as 0, and `make test` went green over them. The driver's own verdict is written down
# inside and read here; no verdict at all (the compositor died, or hit its deadline) is a failure.
verdict() {
  [ -f "$1" ] && return "$(cat "$1")"
  echo "the run ended without a verdict (compositor deadline, or it died)" >&2
  return 124
}

run_in_cage() {
  # cage runs one client, so that client is the script above. The compositor gets a hard deadline
  # of its own: it has been seen to linger after its client exits, and a hung compositor must not
  # hang a test run. The deadline is for a compositor that has stopped answering, not for a slow
  # run: at 789 checks the suite came within a minute of the old 600 and a whole green run was
  # thrown away as "no verdict" (2026-10-06). It is a ceiling, so raising it costs a hung run
  # nothing but the waiting.
  rm -f "$work/rc.cage"
  write_client "$work/rc.cage" "$out/driver.log" "$@"
  timeout -k 5 "${KIKI_E2E_CAGE_TIMEOUT:-1800}" cage -- "$work/client" || true
  verdict "$work/rc.cage"
}

run_in_sway() {
  # sway for the flows that need a layer surface. cage has no `wlr-layer-shell` at all, so a
  # chooser shown under it is drawn nowhere and takes no keys — measured on 2026-10-04 with a
  # bare overlay panel: under cage, two "Failed to initialize layershell integration" and a black
  # screen; under sway, the panel is there, 400x200 at the centre of the output, in its colour.
  #
  # The config is the whole of sway's setup: one headless output the size cage gives, no bar, no
  # decoration, and every window full screen, so the shell fills the output from the origin as it
  # does under cage and the geometry the flows aim at is still the window's own.
  rm -f "$work/rc.sway"
  write_client "$work/rc.sway" "$out/driver-sway.log" "$@"
  cat > "$work/sway.conf" <<EOS
output HEADLESS-1 resolution 1280x720
default_border none
default_floating_border none
focus_follows_mouse no
for_window [title=".*"] fullscreen enable
exec "$work/client"
EOS
  # `setpriv --no-new-privs`: sway is installed with `cap_sys_nice=ep` and asks the kernel for
  # real-time scheduling as it starts. A terminal whose RLIMIT_RTTIME is nought — which is what
  # this desktop hands its terminals — then has sway SIGKILLed fifteen milliseconds in, before it
  # draws anything, and the limit's hard value cannot be raised back by the one who inherited it.
  # Refusing the capability costs a test compositor nothing: it runs at ordinary priority, which
  # is what a test run wants of it anyway.
  # sway's own log goes to the file; the client's stdout — which is the driver's — comes out
  # here, the way cage's does, so a run says what it is doing as it happens.
  SWAYSOCK="$XDG_RUNTIME_DIR/sway.sock" timeout -k 5 "${KIKI_E2E_CAGE_TIMEOUT:-1800}" \
    setpriv --no-new-privs sway -c "$work/sway.conf" 2>"$out/sway.log" || true
  verdict "$work/rc.sway"
}

run_daemon_only() {
  "$work/start-kikid" & kikid_pid=$!
  for _ in $(seq 100); do [ -S "$XDG_RUNTIME_DIR/kiki.sock" ] && break; sleep 0.05; done
  set +e
  python3 -u "$here/driver.py" "$XDG_RUNTIME_DIR/kiki.sock" "$out" --daemon-only "$@" 2>&1 | tee "$out/driver.log"
  rc=${PIPESTATUS[0]}
  set -e
  kill $kikid_pid 2>/dev/null || true
  return $rc
}

run_on_desktop() {
  # The same client script as run_in_cage, on the compositor this shell is already in: for the
  # demo recording (flows/demo.py), which wants the real renderer, the real theme and the real
  # screen. The private runtime directory keeps this daemon and shell apart from the user's own,
  # which means the Wayland socket has to be named by its full path.
  export WAYLAND_DISPLAY="${desktop_display}"
  unset WLR_BACKENDS WLR_LIBINPUT_NO_DEVICES WLR_RENDERER
  # A real pointer (wlrctl's virtual one, which Hyprland serves): the window is full screen at
  # the origin, and Hyprland can say where the cursor is, so aiming corrects itself.
  export KIKI_E2E_POINTER_ORIGIN="0,0"
  export KIKI_E2E_CURSORPOS_CMD="env XDG_RUNTIME_DIR=$KIKI_E2E_REAL_RUNTIME_DIR hyprctl cursorpos"
  # The shell's HOME is the fixture, so the breadcrumb starts at the home icon and the sidebar's
  # home is the demo's; the theme is read from HOME too, so the real one's is linked in.
  mkdir -p "$HOME_FIXTURE/.local/state/omarchy" "$HOME_FIXTURE/.config"
  ln -s "$HOME/.local/state/omarchy/current" "$HOME_FIXTURE/.local/state/omarchy/current" 2>/dev/null || true
  for d in omarchy gtk-3.0 gtk-4.0 fontconfig; do
    [ -e "$HOME/.config/$d" ] && ln -s "$HOME/.config/$d" "$HOME_FIXTURE/.config/$d" 2>/dev/null || true
  done
  bash -c "
    set -e
    '$work/start-kikid' & kikid_pid=\$!
    for i in \$(seq 100); do [ -S '$XDG_RUNTIME_DIR/kiki.sock' ] && break; sleep 0.05; done
    HOME='$HOME_FIXTURE' qs -p '$qs_conf/shell.qml' >'$out/shell.log' 2>&1 & qs_pid=\$!
    for i in \$(seq 200); do qs -p '$qs_conf/shell.qml' ipc call shell state >/dev/null 2>&1 && break; sleep 0.05; done
    python3 -u '$here/driver.py' '$XDG_RUNTIME_DIR/kiki.sock' '$out' $* 2>&1 | tee '$out/driver.log'
    rc=\${PIPESTATUS[0]}
    kill \$qs_pid \$kikid_pid 2>/dev/null || true
    wait \$qs_pid 2>/dev/null || true
    exit \$rc
  "
}

# The flows that need a layer surface, named once in the driver and read back here so the two
# cannot disagree about which they are.
chooser_flows() { python3 "$here/driver.py" --list-part chooser 2>/dev/null; }

# Which compositor the arguments ask for. `--part chooser`, or `--flow` naming nothing but
# chooser flows, is sway's; anything else is cage's. No arguments at all is the whole suite,
# which is both — cage first and then the chooser pass, one after the other, never at once.
wants_sway() {
  case " $* " in *" --part chooser "*) return 0 ;; esac
  local named="" prev="" a
  for a in "$@"; do
    [ "$prev" = "--flow" ] && named="$named $a"
    prev="$a"
  done
  [ -n "$named" ] || return 1
  local list; list=" $(chooser_flows | tr '\n' ' ')"
  for a in $named; do
    case "$list" in *" $a "*) ;; *) return 1 ;; esac
  done
  return 0
}

# A developer without sway still gets the whole cage suite; only the chooser pass stands down,
# and says which package would bring it back.
sway_pass() {
  if ! command -v sway >/dev/null; then
    echo "" >&2
    echo "sway is not installed: the chooser flows (a layer surface, which cage cannot show) did not run." >&2
    echo "  install it with 'sudo pacman -S sway' to cover them too." >&2
    return 0
  fi
  echo ""
  echo "============================================================"
  echo "the chooser flows, under sway (cage has no layer shell)"
  echo "============================================================"
  run_in_sway "$@"
}

if [ -n "${KIKI_E2E_DESKTOP:-}" ]; then
  run_on_desktop "$@"
elif [ "${KIKI_E2E_COMPOSITOR:-}" = "sway" ] || wants_sway "$@"; then
  [ $# -gt 0 ] || set -- --part chooser
  command -v sway >/dev/null || { echo "sway is not installed: 'sudo pacman -S sway'" >&2; exit 2; }
  run_in_sway "$@"
elif command -v cage >/dev/null; then
  if [ $# -gt 0 ]; then
    run_in_cage "$@"
  else
    # The whole suite: everything under cage, then the chooser flows under sway. Both verdicts
    # count — a run is green only when both are — and each block says which compositor it was.
    echo "============================================================"
    echo "the suite, under cage"
    echo "============================================================"
    set +e; run_in_cage; cage_rc=$?
    sway_pass --part chooser; sway_rc=$?
    set -e
    if [ "$cage_rc" -ne 0 ]; then exit "$cage_rc"; fi
    exit "$sway_rc"
  fi
else
  echo "cage is not installed: running the flows that need only the daemon." >&2
  echo "  install it with 'sudo pacman -S cage' to cover the shell flows too." >&2
  run_daemon_only "$@"
fi
