"""Listing a large folder: the numbers plan 01 promises, from the daemon alone."""
import time
from harness import make_tree, wait_for

NEEDS = {"daemon"}
TITLE = "listing a 10k folder"


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"big": {f"file{i}.txt": "" for i in range(1, 10001)}})
    uri = "file://" + root + "/big"

    lid = ctx.lid()
    t0 = time.time()
    d.ok("Open", lid=lid, uri=uri)
    # The scan is asynchronous: the first window is whatever has been scanned so far, so the
    # measurement that matters is how long until there are rows to draw.
    w = wait_for(lambda: (lambda r: r if r["rows"] else None)(d.ok("Window", lid=lid, first=0, count=60)))
    dt = (time.time() - t0) * 1000
    c.check("the first rows arrive within 100 ms", w is not None and dt < 100, f"{dt:.1f} ms")
    c.check("first row is file1.txt, in natural order", w and w["rows"][0]["name"] == "file1.txt", w["rows"][0]["name"] if w else "no rows")

    counted = d.wait_event(lambda e: e.get("event") == "Count" and e.get("lid") == lid and e.get("n") == 10000)
    c.check("the count reaches 10000", counted is not None)
    # The live window's metadata arrives one of two ways: pushed as `Rows` when a stat lands, or
    # already on the rows the scan's `Reset` carries for this window (0.5.0: the first screenful
    # is stated before it goes). Which one is timing — a stat queued by a mid-scan Window lands
    # before or after the sort — so either satisfies this.
    pushed = any(e.get("event") == "Rows" and e.get("lid") == lid for e in d.events)
    carried = any(e.get("event") == "Reset" and e.get("lid") == lid and any(r.get("meta") for r in e.get("rows") or []) for e in d.events)
    c.check("metadata arrives for the live window", pushed or carried)

    r = d.call("Sort", lid=lid, role="size", order="desc")
    c.check("sort by size replies once the rows are enriched", "ok" in r, r)

    # The folder is already open above, so it is in the cache: no Prefetch needed (the request
    # was removed 2026-09-22; a cold open of 20,000 files answers its first rows in 2 ms anyway).
    c.check("a second open is served from the cache", d.ok("Open", lid=ctx.lid(), uri=uri).get("cached") is True)
