"""A move across filesystems is a copy and a delete, and it has to look like a move anyway.

The fixture lives on /tmp; /dev/shm is a different mount, so this is a real cross-device move
rather than a rename.
"""
import os
from harness import snapshot, wait_for

NEEDS = {"daemon"}
TITLE = "moving across filesystems"


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"src": {"a.txt": "alpha", "sub": {"b.txt": "beta"}}})
    other = "/dev/shm/kiki-e2e-%d" % os.getpid()
    os.makedirs(other, exist_ok=True)
    try:
        c.check("the two paths really are different filesystems", os.stat(root).st_dev != os.stat(other).st_dev)
        before = snapshot(root + "/src")

        d.submit({"op": "move", "items": ["file://" + root + "/src"], "dest": "file://" + other})
        moved = snapshot(other + "/src")
        c.check("everything arrives on the other filesystem", moved == before, "differs")
        c.check("and nothing is left behind", not os.path.exists(root + "/src"))

        # Undo copies every byte back the other way, so wait for the whole tree, not the folder.
        d.call("Undo")
        wait_for(lambda: snapshot(root + "/src") == before, timeout=15)
        c.same_tree("undo brings it back across the boundary", before, snapshot(root + "/src"))
        c.check("and takes it off the other filesystem", not os.path.exists(other + "/src"), os.listdir(other))
    finally:
        os.system("rm -rf '%s'" % other)
