"""Compress and extract: the round trip has to land the same tree (plan 05)."""
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
