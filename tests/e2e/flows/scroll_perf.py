"""A hundred thousand files, scrolled top to bottom in the list, icon and column views.

Not in the default run — the fixture is 100,000 files and the flow takes a minute:

    tests/e2e/run.sh --flow scroll_perf

The fixture is built once by `kikid bench gen flat100k` and kept (KIKI_SCROLL_DIR to put it
elsewhere). Each view is scrolled twice: at a pace a hand could make (KIKI_SCROLL_SLOW_MS, 20 s
for the whole folder — 5,000 rows a second) and as a fling no hand makes (KIKI_SCROLL_FAST_MS,
4 s). Every number is printed whether it passes or not. The budgets are wide — the compositor here
is headless and renders in software, so a frame costs what the machine says it costs — and only
catch a collapse. What the numbers are for is the shape: the frame a view cannot make, the rows
that are not there when they are scrolled to, and how long the view takes to fill in at the end.
"""
import json
import os
import shutil
import subprocess
import time

from harness import wait_for

NEEDS = {"shell"}
TITLE = "scrolling 100,000 files in each view, timed"

N = 100_000
FIXTURE = os.environ.get("KIKI_SCROLL_DIR", "/tmp/kiki-perf/flat100k")
SLOW = int(os.environ.get("KIKI_SCROLL_SLOW_MS", "20000"))
FAST = int(os.environ.get("KIKI_SCROLL_FAST_MS", "4000"))


def build_fixture():
    have = len(os.listdir(FIXTURE)) if os.path.isdir(FIXTURE) else 0
    if have >= N:
        return True
    kikid = os.environ.get("KIKID") or shutil.which("kikid")
    if not kikid:
        return False
    os.makedirs(os.path.dirname(FIXTURE) or ".", exist_ok=True)
    t0 = time.monotonic()
    r = subprocess.run([kikid, "bench", "gen", "flat100k", FIXTURE], capture_output=True, text=True, timeout=900)
    print(f"  ... built the fixture in {time.monotonic() - t0:.1f}s")
    return r.returncode == 0


def stats(sh):
    try:
        return json.loads(sh.call("scrollStats") or "{}")
    except json.JSONDecodeError:
        return {}


def scroll(sh, ms):
    sh.call("scrollRun", ms)
    return wait_for(lambda: (lambda s: s if s and not s.get("running", True) else None)(stats(sh)),
                    timeout=ms / 1000 + 30, interval=0.1)


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    if not c.check(f"the fixture has {N} files", build_fixture(), FIXTURE):
        return
    uri = "file://" + FIXTURE
    t0 = time.monotonic()
    sh.call("open", uri)
    got = wait_for(lambda: sh.state().get("count") if sh.state().get("uri") == uri and sh.state().get("done") else None,
                   timeout=60, interval=0.05)
    print(f"  ... listing {got} rows took {(time.monotonic() - t0) * 1000:.0f}ms")
    if not c.check(f"the folder lists all {N} files", got is not None and got >= N, got):
        return

    print(f"  {'view':8} {'pace':>6} {'frames':>7} {'avg':>7} {'p95':>6} {'worst':>6} {'>33ms':>6} {'blank frames':>13} {'settle':>7} {'requests':>9}")
    for view in ("list", "icon", "columns"):
        sh.call("setView", view)
        if wait_for(lambda: sh.state().get("view") == view or None, timeout=10) is None:
            c.check(f"{view}: the view opens", False, sh.state().get("view"))
            continue
        time.sleep(0.5)
        for label, ms in (("slow", SLOW), ("fast", FAST)):
            s = scroll(sh, ms)
            if not c.check(f"{view}, {label}: the scroll finishes", s is not None and "error" not in s, s or stats(sh)):
                continue
            blank = 100 * s["blankFrames"] / max(1, s["frames"])
            print(f"  {view:8} {label:>6} {s['frames']:7d} {s['avgMs']:6.1f}ms {s['p95Ms']:4d}ms {s['worstMs']:4d}ms {s['over33']:6d}"
                  f" {s['blankFrames']:6d} ({blank:3.0f}%) {s['settleMs']:5d}ms {s['requests']:9d}")
            c.check(f"{view}, {label}: it reached the last row", s["lastRow"] >= s["rows"] - 1, (s["lastRow"], s["rows"]))
            c.check(f"{view}, {label}: the view fills in within 2s of stopping", s["settleMs"] < 2000, f"{s['settleMs']}ms")
            c.check(f"{view}, {label}: no frame takes a second", s["worstMs"] < 1000, f"{s['worstMs']}ms")
            if label == "slow":
                # At a pace a hand can make, the rows should be there before they are scrolled to.
                c.check(f"{view}, slow: rows are there when they are scrolled to (under 5% of frames blank)", blank < 5, f"{blank:.1f}%")
                c.check(f"{view}, slow: the median frame is under 50ms", s["avgMs"] < 50, f"{s['avgMs']}ms")
