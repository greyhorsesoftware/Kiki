"""Trash, restore and delete for good, asserted on both the folder and the trash (plan 04)."""
from harness import snapshot, wait_for

NEEDS = {"daemon"}
TITLE = "trash, restore, delete"


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"keep.txt": "keep", "gone.txt": "gone"})

    before = snapshot(root)
    d.submit({"op": "trash", "items": ["file://" + root + "/gone.txt"]})
    c.tree_changed("trash takes it out of the folder", before, snapshot(root), removed=["gone.txt"])
    infos = d.ok("TrashInfo")["items"]
    c.check("the trash records where it came from", any(i["path"].endswith("/gone.txt") for i in infos), infos)

    d.call("Undo")
    wait_for(lambda: "gone.txt" in snapshot(root))
    c.same_tree("undo restores it byte for byte", before, snapshot(root))

    d.submit({"op": "delete", "items": ["file://" + root + "/gone.txt"]})
    after = snapshot(root)
    c.check("delete for good leaves nothing behind", "gone.txt" not in after)
    infos = d.ok("TrashInfo")["items"]
    c.check("and nothing in the trash either", not any(i["path"].endswith("/gone.txt") for i in infos))
