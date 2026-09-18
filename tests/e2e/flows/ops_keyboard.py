"""The same operations by key, so the keymap is covered as well as the menu (plan 28).

Never synthesise a bare Super chord here: Omarchy binds Super and rewrites Super+C/X/V to
Ctrl+C/X/V before a window sees them, so the Ctrl chord is what the user's keypress becomes.
"""
from harness import snapshot, wait_for

NEEDS = {"shell", "keyboard"}
TITLE = "operations from the keyboard"


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    root = ctx.fixture({"src": {"a.txt": "alpha"}, "dst": {}})
    src, dst = root + "/src", root + "/dst"

    # Without this every later check passes by accident: a modal over the window swallows the
    # keys, nothing happens, and "the file is not there" is true because it never arrived.
    sh.open("file://" + src)
    if not c.check("the keymap has the keyboard focus", sh.state().get("keyFocus") is True, sh.state()):
        return

    # Copy and paste: Ctrl+C / Ctrl+V (what Super+C / Super+V arrive as)
    sh.select("a.txt")
    sh.keys(("ctrl", "c"))
    c.check("the clipboard holds the selection", wait_for(lambda: sh.state().get("clipboard")) is not None)
    sh.open("file://" + dst)
    sh.keys(("ctrl", "v"))
    wait_for(lambda: "a.txt" in snapshot(dst))
    c.check("Ctrl+V pastes into the folder on show", "a.txt" in snapshot(dst))

    # Rename: F2, type, Enter. The editor opens asynchronously, so wait for it rather than
    # typing into whatever happens to have the focus.
    sh.select("a.txt")
    sh.keys(("F2",))
    opened = sh.wait_state(lambda st: st.get("renaming", -1) >= 0)
    c.check("F2 opens the inline editor", opened is not None, sh.state())
    sh.type("renamed")          # the editor preselects the stem, so only the stem is retyped
    sh.keys(("Return",))
    wait_for(lambda: "renamed.txt" in snapshot(dst))
    c.check("F2 renames the row in place", "renamed.txt" in snapshot(dst) and "a.txt" not in snapshot(dst), list(snapshot(dst)))

    # New folder: Ctrl+Shift+N
    before = snapshot(dst)
    sh.keys(("ctrl", "shift", "n"))
    wait_for(lambda: "New folder" in snapshot(dst))
    c.tree_changed("Ctrl+Shift+N makes one folder", before, snapshot(dst), added=["New folder"])
    # A new folder lands in the editor waiting for its name, and it opens only once the daemon
    # has reported the folder — so wait for the editor before dismissing it, or the Escape goes
    # out before there is anything to escape and the next keystroke is typing, not a shortcut.
    c.check("the new folder opens its editor", sh.wait_state(lambda st: st.get("renaming", -1) >= 0) is not None, sh.state())
    sh.keys(("Escape",))
    c.check("Escape closes that editor", sh.wait_state(lambda st: st.get("renaming", -1) < 0) is not None, sh.state())

    # Trash and undo: Del, Ctrl+Z. The file has to be there first, or "it is gone" means nothing.
    sh.select("renamed.txt")
    before = snapshot(dst)
    if not c.check("the file to trash is in the folder", "renamed.txt" in before, list(before)):
        return
    sh.keys(("Delete",))
    wait_for(lambda: "renamed.txt" not in snapshot(dst))
    if not c.check("Del moves it to the trash", "renamed.txt" not in snapshot(dst), list(snapshot(dst))):
        return
    sh.keys(("ctrl", "z"))
    wait_for(lambda: "renamed.txt" in snapshot(dst))
    c.same_tree("Ctrl+Z puts it back", before, snapshot(dst))
