"""The Vim letters are the keys (docs/0.2.0/04-keys.md): every one of them pressed in the real
window, and what it did read back from the window and the disk.

h j k l move; v extends; y x p copy cut paste; dd trashes and z brings it back; . shows hidden
files; f filters; r renames; m opens the menu. Bare letters are commands, so a letter never
jumps to a name any more.
"""
import os
from harness import wait_for

NEEDS = {"shell", "keyboard"}
TITLE = "the Vim letters are the keys"


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    root = ctx.fixture({"a.txt": "a", "b.txt": "b", "c.txt": "c", "Projects": {"deep.txt": "d"}, ".hidden": "h"})
    uri = lambda *parts: "file://" + os.path.join(root, *parts)
    sel = lambda: sh.state().get("selection")
    sh.open(uri())
    sh.call("setView", "list")
    if not c.check("a.txt is chosen", sh.select("a.txt") is not None, sh.state()):
        return

    # ---------------------------------------------------------------- h j k l
    sh.keys(("j",))
    c.check("j moves down", sh.wait_state(lambda s: s.get("selection") == [uri("b.txt")], 3) is not None, sel())
    sh.keys(("k",))
    c.check("k moves up", sh.wait_state(lambda s: s.get("selection") == [uri("a.txt")], 3) is not None, sel())
    sh.keys(("k",))
    c.check("the folder is first in the list", sh.wait_state(lambda s: s.get("selection") == [uri("Projects")], 3) is not None, sel())
    sh.keys(("l",))
    c.check("l enters the folder", sh.wait_state(lambda s: s.get("uri") == uri("Projects") and s.get("done"), 5) is not None, sh.state().get("uri"))
    sh.keys(("h",))
    c.check("h goes up", sh.wait_state(lambda s: s.get("uri") == uri() and s.get("done"), 5) is not None, sh.state().get("uri"))

    # ---------------------------------------------------------------- v, then y and p
    sh.select("a.txt")
    sh.keys(("v",), ("j",))
    c.check("v then j extends the selection", sh.wait_state(lambda s: sorted(s.get("selection") or []) == [uri("a.txt"), uri("b.txt")], 3) is not None, sel())
    sh.keys(("Escape",))
    c.check("Esc ends extending and keeps the selection", sorted(sel() or []) == [uri("a.txt"), uri("b.txt")], sel())
    sh.keys(("j",))
    c.check("j after Esc moves alone again", sh.wait_state(lambda s: s.get("selection") == [uri("c.txt")], 3) is not None, sel())
    sh.select("a.txt"); sh.keys(("v",), ("j",), ("y",))
    c.check("y copies the selection", sh.wait_state(lambda s: sorted(s.get("clipboard") or []) == [uri("a.txt"), uri("b.txt")] and not s.get("clipboardCut"), 3) is not None, sh.state().get("clipboard"))
    sh.select("Projects"); sh.keys(("l",))
    sh.wait_state(lambda s: s.get("uri") == uri("Projects") and s.get("done"), 5)
    sh.keys(("p",))
    c.check("p pastes them here", wait_for(lambda: os.path.exists(os.path.join(root, "Projects", "a.txt")) and os.path.exists(os.path.join(root, "Projects", "b.txt")) or None, timeout=10), os.listdir(os.path.join(root, "Projects")))
    sh.keys(("h",))
    sh.wait_state(lambda s: s.get("uri") == uri() and s.get("done"), 5)

    # ---------------------------------------------------------------- x
    sh.select("b.txt"); sh.keys(("x",))
    c.check("x cuts", sh.wait_state(lambda s: s.get("clipboard") == [uri("b.txt")] and s.get("clipboardCut"), 3) is not None, sh.state())

    # ---------------------------------------------------------------- dd, z
    sh.select("c.txt"); sh.keys(("d",), ("d",))
    c.check("dd trashes", wait_for(lambda: (not os.path.exists(os.path.join(root, "c.txt"))) or None, timeout=10), os.listdir(root))
    sh.keys(("z",))
    c.check("z brings it back", wait_for(lambda: os.path.exists(os.path.join(root, "c.txt")) or None, timeout=10), os.listdir(root))
    sh.select("c.txt"); sh.keys(("d",))
    sh.keys(("j",))
    sh.keys(("d",))
    c.check("one d, something else, one d: nothing trashed", os.path.exists(os.path.join(root, "c.txt")), os.listdir(root))

    # ---------------------------------------------------------------- . f : m r
    before = sh.state().get("count")
    sh.keys(("period",))
    c.check(". shows hidden files", sh.wait_state(lambda s: s.get("count") == before + 1 and s.get("done"), 5) is not None, sh.state().get("count"))
    sh.keys(("period",))
    c.check(". again hides them", sh.wait_state(lambda s: s.get("count") == before and s.get("done"), 5) is not None, sh.state().get("count"))
    sh.keys(("f",))
    c.check("f opens the filter", sh.wait_state(lambda s: s.get("filterOpen") is True, 3) is not None, sh.state().get("filterOpen"))
    sh.keys(("Escape",))
    sh.wait_state(lambda s: s.get("filterOpen") is False, 3)
    sh.keys(("m",))
    c.check("m opens the menu", sh.wait_state(lambda s: s.get("menuVisible") is True, 3) is not None, sh.state().get("menuVisible"))
    sh.keys(("Escape",))
    sh.wait_state(lambda s: s.get("menuVisible") is False, 3)
    sh.select("a.txt"); sh.keys(("r",))
    c.check("r renames", sh.wait_state(lambda s: s.get("renaming"), 3) is not None, sh.state().get("renaming"))
    sh.keys(("Escape",))
    sh.wait_state(lambda s: not s.get("renaming"), 3)
    # A letter is a command, not a jump: c does nothing to the selection.
    sh.select("a.txt"); sh.keys(("c",))
    c.check("a bare letter no longer jumps to a name", sel() == [uri("a.txt")], sel())
