"""A thousand photographs in the gallery: how long each part of it takes.

Not in the default run — the fixture is a gigabyte of JPEGs and the flow takes a minute:

    tests/e2e/run.sh --flow gallery_perf

The fixture is built once by `kikid bench gen gallery1k` and kept between runs (set
KIKI_PERF_DIR to put it somewhere else, KIKI_PERF_N for fewer pictures). Every number is
printed whether it passes or not; the checks are wide budgets that only catch a collapse, since
the machine underneath is whatever it is. What the numbers are for is the shape: which part of
showing a picture costs what.
"""
import json
import os
import shutil
import subprocess
import time

from harness import wait_for

NEEDS = {"shell"}
TITLE = "a thousand-photograph gallery, timed"

N = int(os.environ.get("KIKI_PERF_N", "1000"))
FIXTURE = os.environ.get("KIKI_PERF_DIR", "/tmp/kiki-perf/gallery1k")
STEPS = int(os.environ.get("KIKI_PERF_STEPS", "30"))


def build_fixture():
    """Generate the photographs once. Already there and full? Leave them alone."""
    have = len([f for f in os.listdir(FIXTURE)]) if os.path.isdir(FIXTURE) else 0
    if have >= N:
        return True
    kikid = os.environ.get("KIKID") or shutil.which("kikid")
    if not kikid:
        return False
    os.makedirs(os.path.dirname(FIXTURE) or ".", exist_ok=True)
    t0 = time.monotonic()
    r = subprocess.run([kikid, "bench", "gen", "gallery1k", FIXTURE], capture_output=True, text=True, timeout=900)
    print(f"  ... built the fixture in {time.monotonic() - t0:.1f}s")
    return r.returncode == 0


def rss_kb(pattern):
    """Resident memory of the newest process matching a command line, in KB; 0 when there is
    none. Matched on the whole command line: every `qs ipc` call is itself a process called qs,
    so a name alone would measure the driver rather than the window."""
    try:
        pid = subprocess.run(["pgrep", "-n", "-f", pattern], capture_output=True, text=True).stdout.strip()
        if not pid:
            return 0
        with open(f"/proc/{pid}/status") as fh:
            for line in fh:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
    except OSError:
        pass
    return 0


def thumbs_on_disk():
    root = os.environ.get("KIKI_THUMB_DIR") or os.path.join(
        os.environ.get("XDG_CACHE_HOME") or os.path.expanduser("~/.cache"), "thumbnails")
    n = 0
    for sub in ("normal", "large"):
        d = os.path.join(root, sub)
        if os.path.isdir(d):
            n += len(os.listdir(d))
    return n


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    if not c.check(f"the fixture has {N} photographs", build_fixture(), FIXTURE):
        return

    # What one `qs ipc` round trip costs, so the numbers below can be read net of the driver.
    t0 = time.monotonic()
    for _ in range(5):
        sh.state()
    ipc_ms = (time.monotonic() - t0) * 1000 / 5
    print(f"  ... one ipc round trip is {ipc_ms:.0f}ms; every wall-clock figure below includes one")

    base_rss_d, base_rss_s = rss_kb("kikid"), rss_kb("shell.qml")
    thumbs_before = thumbs_on_disk()

    # ------------------------------------------------------------------ listing
    uri = "file://" + FIXTURE
    t0 = time.monotonic()
    sh.call("open", uri)
    got = wait_for(lambda: sh.state().get("count") if sh.state().get("uri") == uri and sh.state().get("done") else None,
                   timeout=60, interval=0.02)
    listing_ms = (time.monotonic() - t0) * 1000
    print(f"  ... listing {got} rows took {listing_ms:.0f}ms")
    c.check(f"the folder lists all {N} photographs", got == N, got)
    c.check("listing a thousand files stays under 5s", listing_ms < 5000, f"{listing_ms:.0f}ms")
    rss_listed = rss_kb("shell.qml")
    print(f"  ... holding the listing cost the window {(rss_listed - base_rss_s) // 1024}MB")

    # ------------------------------------------------------- the first picture
    t0 = time.monotonic()
    sh.call("setView", "gallery")
    first = wait_for(lambda: ready(stats(sh)), timeout=60, interval=0.02)
    first_ms = (time.monotonic() - t0) * 1000
    print(f"  ... the first picture was on screen {first_ms:.0f}ms after the view opened"
          f" ({(first or {}).get('lastMs', 0)}ms of it decoding)")
    c.check("the first picture arrives within 5s", first is not None and first_ms < 5000, f"{first_ms:.0f}ms")

    # --------------------------------------------------------------- stepping
    before = stats(sh)
    walls = []
    for _ in range(STEPS):
        n = stats(sh).get("decodes", 0)
        t = time.monotonic()
        sh.call("gallery", "next")
        wait_for(lambda: stats(sh).get("decodes", 0) > n or None, timeout=30, interval=0.005)
        walls.append((time.monotonic() - t) * 1000)
    after = stats(sh)
    decoded = after.get("decodes", 0) - before.get("decodes", 0)
    walls.sort()
    median = walls[len(walls) // 2] if walls else 0
    print(f"  ... {decoded} steps: {median:.0f}ms median wall, {walls[-1]:.0f}ms worst;"
          f" the window says {after.get('avgMs')}ms average decode, {after.get('worstMs')}ms worst")
    c.check("stepping decodes one picture per step", decoded == STEPS, decoded)
    c.check("the median step stays under 1.5s", median < 1500, f"{median:.0f}ms")

    # ------------------------------------------------------ jumping and thumbs
    # Landing in the middle is the worst case for the filmstrip: nothing around it is thumbnailed
    # and the daemon is asked for a screenful at once.
    far = f"DSC_{N // 2:05d}.jpg"
    t0 = time.monotonic()
    sh.call("select", far)
    jump = wait_for(lambda: ready(stats(sh), N // 2), timeout=60, interval=0.02)
    jump_ms = (time.monotonic() - t0) * 1000
    print(f"  ... jumping to row {N // 2} put a picture up in {jump_ms:.0f}ms")
    c.check("a jump into the middle shows its picture within 5s", jump is not None and jump_ms < 5000, f"{jump_ms:.0f}ms")

    made = thumbs_on_disk() - thumbs_before
    print(f"  ... browsing has had {made} thumbnails made so far — the filmstrip asks for what it shows")
    c.check("the filmstrip is having thumbnails made for it", made > 0, made)

    # How fast they can be made when something does ask for a lot of them at once. Through the
    # daemon rather than the window: the strip only ever asks for a screenful, so browsing says
    # nothing about the ceiling.
    daemon = getattr(ctx, "daemon", None)
    if daemon is not None:
        lid = ctx.lid()
        daemon.ok("Open", lid=lid, uri=uri)
        start = thumbs_on_disk()
        t0 = time.monotonic()
        w = wait_for(lambda: (lambda r: r if r["rows"] else None)(daemon.ok("Window", lid=lid, first=0, count=512)), timeout=30)
        # Only the rows that came back without one are work; the rest were made while browsing.
        missing = sum(1 for r in w["rows"] if not r.get("thumb")) if w else 0
        wait_for(lambda: (thumbs_on_disk() - start >= missing) or None, timeout=180, interval=0.05)
        made, took = thumbs_on_disk() - start, time.monotonic() - t0
        rate = made / max(1e-9, took)
        print(f"  ... {missing} thumbnails asked for at once: {made} made in {took:.1f}s = {rate:.0f}/s")
        c.check("thumbnails are made at more than 20 a second", missing == 0 or rate > 20, f"{rate:.0f}/s")

    # ------------------------------------------------------------------ memory
    rss_d, rss_s = rss_kb("kikid"), rss_kb("shell.qml")
    print(f"  ... kikid {base_rss_d // 1024}MB → {rss_d // 1024}MB, window {base_rss_s // 1024}MB → {rss_s // 1024}MB")
    c.check("the daemon does not hold a thousand photographs in memory", rss_d < 600 * 1024, f"{rss_d // 1024}MB")
    c.check("nor does the window", rss_s < 1500 * 1024, f"{rss_s // 1024}MB")


def ready(st, at=None):
    """The stats when a picture is up (and, when asked, when it is the row wanted)."""
    if not st.get("ready"):
        return None
    return st if at is None or st.get("current") == at else None


def stats(sh):
    try:
        return json.loads(sh.call("galleryStats") or "{}")
    except json.JSONDecodeError:
        return {}
