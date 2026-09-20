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
