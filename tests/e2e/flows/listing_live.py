"""A folder that changes while it is open: what the daemon tells a window, and what it keeps.

Plan 31, phase 4a. A file arriving or leaving is told as what happened (`Splice`), not as "forget
everything and ask again" (`Reset`); a folder read again (a rescan) still knows every thumbnail
it knew; and every answer carries the number of the view it describes.
"""
import os
import signal
import struct
import subprocess
import time
import zlib

from harness import wait_for

NEEDS = {"daemon"}
TITLE = "a folder changing while it is open"


def png(path, shade):
    """A 32×32 grey PNG: enough for the thumbnailer to have something to decode."""
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    raw = b"".join(b"\0" + bytes([shade]) * 32 for _ in range(32))
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 32, 32, 8, 0, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"pics": {}})
    folder = os.path.join(root, "pics")
    names = [f"p{i}.png" for i in range(1, 7)]
    for i, n in enumerate(names):
        png(os.path.join(folder, n), 40 * i)
    uri = "file://" + folder
    lid = ctx.lid()
    d.ok("Open", lid=lid, uri=uri)

    def window():
        return d.ok("Window", lid=lid, first=0, count=50)

    def thumbs(w):
        return {r["name"]: r["thumb"] for r in w["rows"]}

    full = wait_for(lambda: (lambda w: w if len(w["rows"]) == 6 and all(thumbs(w).values()) else None)(window()), timeout=20)
    c.check("every picture gets its thumbnail", full is not None, thumbs(window()))
    if full is None:
        return
    c.check("an answer says which view it describes", isinstance(full.get("gen"), int), full.get("gen"))
    gen = full["gen"]

    # ---------------------------------------------------------------- one file arrives
    d.events.clear()
    png(os.path.join(folder, "p3b.png"), 200)
    sp = d.wait_event(lambda e: e.get("event") in ("Splice", "Reset") and e.get("lid") == lid, timeout=5)
    c.check("a file arriving is a Splice, not a Reset", sp is not None and sp["event"] == "Splice", sp)
    if sp and sp["event"] == "Splice":
        op = sp["ops"][0]
        c.check("…an insert, where the sort puts it, with its row", (op["op"], op["pos"], op["row"]["name"]) == ("insert", 3, "p3b.png"), op)
        c.check("…numbered one past the view it changes", sp["gen"] == gen + 1 and sp["n"] == 7, (sp["gen"], sp["n"]))
    w = window()
    c.check("nobody else lost their thumbnail to it", all(thumbs(w)[n] for n in names), thumbs(w))
    c.check("the window agrees with the splice", [r["name"] for r in w["rows"]][:5] == ["p1.png", "p2.png", "p3.png", "p3b.png", "p4.png"] and w["gen"] == gen + 1, w["gen"])

    # ---------------------------------------------------------------- one file leaves
    d.events.clear()
    os.remove(os.path.join(folder, "p1.png"))
    sp = d.wait_event(lambda e: e.get("event") in ("Splice", "Reset") and e.get("lid") == lid, timeout=5)
    c.check("a file leaving is a Splice too", sp is not None and sp["event"] == "Splice" and sp["ops"] == [{"op": "remove", "pos": 0}], sp)

    # ---------------------------------------------------------------- the folder is read again
    wait_for(lambda: all(thumbs(window()).values()) or None, timeout=20)
    d.events.clear()
    d.ok("Refresh", lid=lid)
    rs = d.wait_event(lambda e: e.get("event") == "Reset" and e.get("lid") == lid, timeout=5)
    c.check("a rescan is a Reset, numbered", rs is not None and isinstance(rs.get("gen"), int), rs)
    w = window()   # at once: no waiting for thumbnail jobs to run again
    c.check("the very next window still has every thumbnail", len(w["rows"]) == 6 and all(thumbs(w).values()), thumbs(w))

    # ---------------------------------------------------------------- a file changes
    before = thumbs(w)["p2.png"]
    d.events.clear()
    png(os.path.join(folder, "p2.png"), 250)
    os.utime(os.path.join(folder, "p2.png"), (2_000_000_000, 2_000_000_000))
    again = wait_for(lambda: (lambda t: t if t else None)(thumbs(window()).get("p2.png")), timeout=20)
    c.check("a changed picture is thumbnailed again, the others left alone", again is not None and all(thumbs(window()).values()), (before, again))
    c.check("…and changing it reset nobody", not any(e.get("event") == "Reset" and e.get("lid") == lid for e in d.events))

    # ---------------------------------------------------------------- the thumbnailer dies
    # Decoding a picture is the one thing kiki does to a file's contents, and it happens in
    # `kiki-thumber` so that a file which kills the decoder costs that file its thumbnail and
    # nothing else. Kill it under a running daemon and the folder must carry on.
    def thumber_pids():
        out = subprocess.run(["pgrep", "-f", "kiki-thumber"], capture_output=True, text=True).stdout
        return [int(x) for x in out.split()]

    pids = thumber_pids()
    c.check("thumbnails are made in a process of their own", bool(pids), pids)
    if pids:
        for pid in pids:
            os.kill(pid, signal.SIGKILL)
        time.sleep(0.5)
        # The daemon is still there and still answering about this folder.
        w = window()
        c.check("the daemon outlives its thumbnailer", len(w["rows"]) == 6, w.get("rows"))
        c.check("…and what was already made is still shown", all(thumbs(w).values()), thumbs(w))
        # A new picture still gets one: the thumbnailer was started again.
        png(os.path.join(folder, "p9.png"), 90)
        got = wait_for(lambda: thumbs(window()).get("p9.png") or None, timeout=20)
        c.check("a picture wanted after the death is thumbnailed", bool(got), got)
        c.check("…by a thumbnailer that was started again", bool(thumber_pids()), thumber_pids())
