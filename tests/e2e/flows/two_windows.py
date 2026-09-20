"""What one window does, another hears about (plan 01: "two clients both receive events").

The shell has always listened for `FavoritesChanged` and `Gone`; the daemon never sent either, and
`LocationsChanged` went only to the window that made the change. So a second window's sidebar went
stale, and a window showing a folder that had just been deleted went on showing its rows.
"""
import os, shutil, sys
from harness import Daemon, wait_for

NEEDS = {"daemon"}
TITLE = "a second window hears what the first one did"


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    other = Daemon(sys.argv[1], client="e2e-other")
    root = ctx.fixture({"doomed": {"a.txt": "a"}, "kept": {}})

    was = d.ok("Favorites")["items"]
    other.drain(0.1); other.events.clear()
    d.ok("SetFavorites", items=was + [{"name": "kept", "uri": "file://" + root + "/kept"}])
    told = other.wait_event(lambda e: e.get("event") == "FavoritesChanged", timeout=3)
    c.check("a favourite added in one window is announced to the other", told is not None)
    c.check("and the other reads the same list", any(f.get("uri", "").endswith("/kept") for f in other.ok("Favorites")["items"]))
    d.ok("SetFavorites", items=was)

    # Both windows are showing a folder; it is deleted from outside.
    uri = "file://" + root + "/doomed"
    l1, l2 = ctx.lid(), 77
    d.ok("Open", lid=l1, uri=uri)
    other.ok("Open", lid=l2, uri=uri)
    wait_for(lambda: d.ok("Window", lid=l1, first=0, count=10)["rows"] or None)
    other.ok("Window", lid=l2, first=0, count=10)
    d.events.clear(); other.events.clear()
    shutil.rmtree(os.path.join(root, "doomed"))
    g1 = d.wait_event(lambda e: e.get("event") == "Gone" and e.get("lid") == l1, timeout=3)
    g2 = other.wait_event(lambda e: e.get("event") == "Gone" and e.get("lid") == l2, timeout=3)
    c.check("a folder deleted from under a window: that window is told it is gone", g1 is not None, [e.get("event") for e in d.events][-8:])
    c.check("and so is every other window showing it", g2 is not None)

    # A client built for another protocol is told so at the door.
    stranger = Daemon.__new__(Daemon)
    import socket
    stranger.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); stranger.s.connect(sys.argv[1])
    stranger.buf, stranger.n, stranger.events = b"", 1, []
    r = stranger.call("Hello", version=99, client="from-the-future")
    c.check("a client speaking another protocol version is refused, and told why", "err" in r and r["err"].get("code") == "Version" and "protocol" in r["err"].get("message", ""), r)
    r = stranger.call("Hello", client="a-script")
    c.check("one that names no version is let in", "ok" in r, r)
