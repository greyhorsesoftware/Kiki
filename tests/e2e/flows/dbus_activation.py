"""kiki's face on the session bus, with a real window behind it (docs/0.3.0/01-daemon-on-demand.md).

Another application asks the bus for `org.freedesktop.FileManager1` or the file-chooser portal;
the bus starts `kiki-dbus` from a service file; the listener puts the request to a kiki window
through its IPC and carries the answer back. `plugins/kiki-plugin-dbus/tests/activation.rs`
proves that round trip against a stub window — this proves it against the real one: the folder
really opens, with the file selected.

The chooser is not here: it became a window of its own on 2026-10-04 — a layer surface, which
`cage` cannot show (docs/0.5.0/11-chooser-window.md) — so everything that drives a chooser is in
`flows/chooser.py`, which runs under sway. What is left is what needs no chooser, and it is what
CI runs: the bus starting the listener, and "Show in folder" reaching the window.

The bus here is private (`dbus-run-session`) and can see nothing but this test's service file,
which is also how a person takes the FileManager1 name from the package that owns it system-wide
(Settings -> Omarchy writes the same file under ~/.local/share).
"""
import json
import os
import shutil
import subprocess
import time

from harness import wait_for

NEEDS = {"shell"}
TITLE = "the session bus starts kiki's listener, which reaches the window"

LISTENER = "kiki-plugin-dbus"   # what it is built as; the package installs it as kiki-dbus


def tools():
    return all(shutil.which(t) for t in ("dbus-run-session", "busctl"))


def listener_path(ctx):
    """The listener built for this run, beside the daemon the harness is driving."""
    for d in (os.environ.get("KIKI_PLUGIN_DIR", ""), os.path.dirname(os.environ.get("KIKI_DAEMON", "")), "target/release"):
        if d:
            p = os.path.join(d, LISTENER)
            if os.path.exists(p):
                return p
    return None


def bus_env(ctx, services):
    """An environment whose session bus can only activate what `services` holds.

    XDG_DATA_HOME as well as XDG_DATA_DIRS, and it matters: the developer's own
    ~/.local/share/dbus-1/services outranks XDG_DATA_DIRS, and once the owner had turned kiki's
    "Show in folder" row on, this flow was starting the INSTALLED listener instead of the one
    under test and failing on it (2026-09-26). A test must not depend on the machine's home."""
    env = dict(os.environ)
    env["XDG_DATA_DIRS"] = services
    home = os.path.join(services, "home")
    os.makedirs(home, exist_ok=True)
    env["XDG_DATA_HOME"] = home
    return env


def service_file(dirpath, name, exec_path):
    services = os.path.join(dirpath, "dbus-1", "services")
    os.makedirs(services, exist_ok=True)
    with open(os.path.join(services, f"{name}.service"), "w") as f:
        f.write(f"[D-BUS Service]\nName={name}\nExec={exec_path}\n")


def wrapper(dirpath, listener):
    """What the bus starts: the listener, with its own words kept. A listener that cannot reach a
    window says so there, and a failing check prints it — the bus swallows stderr otherwise."""
    path = os.path.join(dirpath, "start-listener")
    log = os.path.join(dirpath, "listener.log")
    pids = os.path.join(dirpath, "listener.pids")
    with open(path, "w") as f:
        # BOTH streams into the log, not only stderr: a listener that keeps the caller's stdout
        # open holds `subprocess.run` at EOF long after the call it was asked about was answered,
        # which reads exactly like a call that never returned (2026-09-26). And a short idle, so
        # the flow does not wait out the minute a real listener would.
        # `$$` before `exec` is the listener's own pid: the check at the end watches those, not a
        # name — `pgrep -f` on the binary's name once matched the shell that had started the
        # whole test run, whose command line happened to mention it.
        # The moment the bus ran it, for a failure to say whether the bus was slow to start the
        # listener or the listener slow to answer.
        f.write(f"#!/bin/sh\necho $$ >> '{pids}'\necho \"started at $(date +%s.%N)\" >> '{log}'\nexport KIKI_DBUS_IDLE_MS=800\nexec '{listener}' >>'{log}' 2>&1\n")
    os.chmod(path, 0o755)
    return path, log, pids


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    if not tools():
        print("  ... dbus-run-session or busctl missing; skipped")
        return
    listener = listener_path(ctx)
    if not listener:
        print(f"  ... {LISTENER} was not built; skipped")
        return

    root = ctx.fixture({"Papers": {"notes.txt": "hello", "other.txt": "x"}})
    data = os.path.join(ctx.base, "dbus-activation")
    os.makedirs(data, exist_ok=True)
    start, listener_log, listener_pids = wrapper(data, listener)
    for name in ("org.freedesktop.FileManager1", "org.freedesktop.impl.portal.desktop.kiki"):
        service_file(data, name, start)
    env = bus_env(ctx, data)

    def said():
        """What the listener made of it, for a check that failed."""
        try:
            return open(listener_log).read().strip()[-600:] or "(the listener said nothing)"
        except OSError:
            return "(the listener was never started)"

    # Somewhere else entirely, so opening the paper is a move the window would not make by itself.
    sh.open("file://" + root)
    sh.wait_state(lambda s: s.get("uri") == "file://" + root and s.get("done"))

    # ---------------------------------------------------------------- the window answers at all
    # What the listener will do, done here first: if this fails the fault is the window's IPC and
    # not the bus, and the checks below would only be a slower way of saying so.
    paper = os.path.join(root, "Papers", "notes.txt")
    conf = os.path.join(os.environ.get("KIKI_SHELL_DIR", ""), "shell.qml")
    direct = subprocess.run(["qs", "-p", conf, "ipc", "call", "shell", "dbus", "ShowItems",
                             json.dumps({"uris": ["file://" + paper]})], capture_output=True, text=True, timeout=30)
    c.check("the window answers the ipc the listener uses", direct.returncode == 0, (direct.returncode, direct.stderr.strip()[-200:]))
    sh.open("file://" + root)
    sh.wait_state(lambda s: s.get("uri") == "file://" + root and s.get("done"))

    # ---------------------------------------------------------------- Show in folder
    called_at = time.time()
    try:
        r = subprocess.run(
            ["dbus-run-session", "--", "busctl", "--user", "call",
             "org.freedesktop.FileManager1", "/org/freedesktop/FileManager1", "org.freedesktop.FileManager1",
             "ShowItems", "ass", "1", "file://" + paper, ""],
            env=env, capture_output=True, text=True, timeout=45,
        )
        took = r.returncode == 0
        why = r.stderr.strip()[-200:]
    except subprocess.TimeoutExpired:
        # Said rather than raised: a call that never returns is this test's finding, and what the
        # listener made of it is the evidence.
        took, why = False, "the call never returned"
    c.check("the bus starts the listener and it takes the call", took,
            (why, f"called at {called_at:.3f}, {time.time() - called_at:.1f}s ago", said()))
    went = sh.wait_state(lambda s: s.get("uri") == "file://" + os.path.join(root, "Papers") and s.get("done"), 20)
    c.check("the window opens the file's folder", went is not None, (sh.state().get("uri"), said()))
    picked = sh.wait_state(lambda s: s.get("selection") == ["file://" + paper], 10)
    c.check("with the file itself selected", picked is not None, sh.state().get("selection"))

    # ---------------------------------------------------------------- nothing left behind
    # The listeners this flow had the bus start serve their request and go; they are not daemons
    # and hold nothing.
    def alive():
        try:
            pids = [p for p in open(listener_pids).read().split() if p]
        except OSError:
            return []
        return [p for p in pids if subprocess.run(["kill", "-0", p], capture_output=True).returncode == 0]
    left = wait_for(lambda: (not alive()) or None, timeout=15)
    c.check("the listeners do not linger once their requests are done", left is not None, alive())
