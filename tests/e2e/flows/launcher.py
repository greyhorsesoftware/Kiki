"""The `kiki` command, run the way a desktop entry or `xdg-open` runs it (plan 09).

With kiki already up, a second launch must hand its folder to the running window and start
nothing. Both halves were broken at once: the launcher looked for the running instance under a
config name the package never installs, so it never found it, and it passed the folder as an
argument the shell never reads — every launch was another window, showing home.
"""
import os, subprocess
from harness import wait_for

NEEDS = {"daemon", "shell"}
TITLE = "a second launch of kiki goes to the window that is already open"

HERE = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
LAUNCHER = os.path.join(HERE, "packaging", "bin", "kiki")


def instances():
    out = subprocess.run(["qs", "list", "--all"], capture_output=True, text=True).stdout
    return out.count("Process ID:")


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    root = ctx.fixture({"inbox": {"a.txt": "a"}, "with space": {"b.txt": "b"}})
    before = instances()

    r = subprocess.run([LAUNCHER, os.path.join(root, "inbox")], capture_output=True, text=True, timeout=20)
    c.check("the launcher returns at once instead of becoming a second kiki", r.returncode == 0, r.stderr)
    went = sh.wait_state(lambda s: s.get("uri") == "file://" + root + "/inbox" and s.get("done"))
    c.check("a path is opened in the running window", went is not None, sh.state().get("uri"))
    c.check("and no second instance was started", instances() == before, f"{before} before, {instances()} after")

    subprocess.run([LAUNCHER, "file://" + root + "/with%20space"], capture_output=True, timeout=20)
    went = sh.wait_state(lambda s: s.get("uri") == "file://" + root + "/with%20space" and s.get("done"))
    c.check("a URI is opened as it is", went is not None, sh.state().get("uri"))

    subprocess.run([LAUNCHER, "inbox"], capture_output=True, timeout=20, cwd=root)
    went = sh.wait_state(lambda s: s.get("uri") == "file://" + root + "/inbox" and s.get("done"))
    c.check("a relative path is taken from where the command was run", went is not None, sh.state().get("uri"))
    c.check("still one kiki", instances() == before, instances())

    # ---------------------------------------------------------------- no units to tend
    # Since 0.3.0 the launcher starts a window and nothing else: the window starts its own daemon
    # and that daemon leaves with it (docs/0.3.0/01-daemon-on-demand.md). It used to reload and
    # restart systemd units on the user's behalf, which is the layering this release removed.
    # Stubs stand in for systemctl and qs, so nothing of the machine's is touched.
    import stat, tempfile
    stubs = tempfile.mkdtemp(prefix="kiki-launcher-")
    bin_ = os.path.join(stubs, "bin"); os.makedirs(bin_)
    calls = os.path.join(stubs, "calls")
    for name in ("systemctl", "qs"):
        path = os.path.join(bin_, name)
        # qs exits 1 for the `ipc call ... present` probe, so the launcher takes the "no window
        # yet" path and starts one; everything either is asked to run is recorded.
        body = f'echo "{name} $*" >> "{calls}"\n' + ('[ "$2" = "ipc" ] || [ "$3" = "ipc" ] && exit 1\nexit 0\n' if name == "qs" else "exit 0\n")
        open(path, "w").write("#!/bin/sh\n" + body)
        os.chmod(path, os.stat(path).st_mode | stat.S_IEXEC)
    env = dict(os.environ, PATH=bin_ + ":" + os.environ.get("PATH", ""), XDG_RUNTIME_DIR=stubs, HOME=stubs)
    env.pop("KIKI_SHELL_DIR", None)
    subprocess.run(["sh", LAUNCHER], env=env, capture_output=True, timeout=20)
    got = open(calls).read().splitlines() if os.path.exists(calls) else []
    c.check("the launcher tends no systemd units at all", not any(l.startswith("systemctl") for l in got), got)
    c.check("it starts a window", any(l.startswith("qs") and "shell.qml" in l for l in got), got)
    import shutil as _sh; _sh.rmtree(stubs, ignore_errors=True)
