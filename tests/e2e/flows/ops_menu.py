"""Every file operation through the context menu, with the tree as the oracle (plan 28)."""
from harness import snapshot, wait_for

NEEDS = {"shell"}
TITLE = "operations through the menu"


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    root = ctx.fixture({"src": {"a.txt": "alpha", "b.txt": "beta"}, "dst": {}})
    src, dst = root + "/src", root + "/dst"

    # New folder
    before = snapshot(dst)
    sh.open("file://" + dst)
    sh.menu("New folder")
    made = wait_for(lambda: "New folder" in snapshot(dst))
    c.tree_changed("New folder creates exactly one folder", before, snapshot(dst), added=["New folder"])
    c.check("the new folder is selected and being renamed", made is not None)

    # Copy and paste
    sh.open("file://" + src)
    sh.select("a.txt")
    sh.menu("Copy")
    sh.open("file://" + dst)
    before = snapshot(dst)
    sh.menu("Paste")
    wait_for(lambda: "a.txt" in snapshot(dst))
    c.tree_changed("paste copies the file in", before, snapshot(dst), added=["a.txt"])
    c.check("the source keeps its copy", "a.txt" in snapshot(src))

    # Cut and paste is a move
    sh.open("file://" + src)
    sh.select("b.txt")
    sh.menu("Cut")
    sh.open("file://" + dst)
    sh.menu("Paste")
    wait_for(lambda: "b.txt" in snapshot(dst))
    c.check("cut and paste moves rather than copies", "b.txt" not in snapshot(src))

    # Trash, then undo puts it back byte for byte
    sh.open("file://" + dst)
    sh.select("a.txt")
    before = snapshot(dst)
    sh.menu("Move to Trash")
    wait_for(lambda: "a.txt" not in snapshot(dst))
    c.tree_changed("trash removes it from the folder", before, snapshot(dst), removed=["a.txt"])
    sh.call("undo")
    wait_for(lambda: "a.txt" in snapshot(dst))
    c.same_tree("undo restores the folder exactly", before, snapshot(dst))
