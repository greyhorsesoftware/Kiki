"""Compress and extract: the round trip has to land the same tree (plan 05)."""
import os
from harness import snapshot, wait_for

NEEDS = {"daemon"}
TITLE = "compress and extract"


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"work": {"one.txt": "one", "two.txt": "two", "sub": {"three.txt": "three"}}, "out": {}})
    work, out = root + "/work", root + "/out"

    original = snapshot(work)
    d.submit({"op": "compress", "items": ["file://" + work], "archive": "file://" + root + "/work.tar.gz", "format": "tar.gz"})
    c.check("the archive is written", "work.tar.gz" in snapshot(root))

    n = d.ok("Preview", uri="file://" + root + "/work.tar.gz").get("n", 0)
    c.check("its members can be previewed", n >= 3, n)

    d.submit({"op": "extract", "archive": "file://" + root + "/work.tar.gz", "dest": "file://" + out})
    wait_for(lambda: snapshot(out))
    extracted = snapshot(out + "/work") if "work" in snapshot(out) else snapshot(out)
    c.same_tree("what comes out is what went in", original, extracted)

    # Several things at the top of an archive go into a folder named after it, not loose into the
    # destination; and into a folder that already holds the user's own files it is a NEW folder,
    # so undo takes back the extraction and nothing else.
    for name, text in (("a.txt", "a"), ("b.txt", "b")):
        with open(os.path.join(root, name), "w") as f:
            f.write(text)
    d.submit({"op": "compress", "items": ["file://" + root + "/a.txt", "file://" + root + "/b.txt"], "archive": "file://" + root + "/pair.zip", "format": "zip"})
    mine = os.path.join(root, "mine")
    os.makedirs(os.path.join(mine, "pair"))
    with open(os.path.join(mine, "pair", "keep.txt"), "w") as f:
        f.write("the user's own")
    before = snapshot(mine)
    d.submit({"op": "extract", "archive": "file://" + root + "/pair.zip", "dest": "file://" + mine})
    c.tree_changed("two loose files land in a folder named after the archive, beside the one already called that", before, snapshot(mine),
                   added=["pair (2)", "pair (2)/a.txt", "pair (2)/b.txt"])
    d.call("Undo")
    got = wait_for(lambda: snapshot(mine) == before or None)
    c.check("undo removes what was extracted and nothing that was there before", got, sorted(snapshot(mine)))

    # An archive with an entry that climbs out of its folder is refused, by name, and writes nothing.
    import subprocess
    os.makedirs(os.path.join(root, "deep", "er"))
    subprocess.run(["bsdtar", "-cPf", "../../evil.tar", "../../a.txt"], cwd=os.path.join(root, "deep", "er"), check=True)
    trap = os.path.join(root, "trap")
    job = d.ok("Submit", op={"op": "extract", "archive": "file://" + root + "/evil.tar", "dest": "file://" + trap})["job"]
    ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("done", "failed", "cancelled"))
    c.check("an archive with a ../ entry fails", ended and ended["job"]["state"] == "failed" and "unsafe" in (ended["job"]["error"] or ""), ended and ended["job"])
    c.check("and nothing was written, not even the destination", not os.path.exists(trap))
