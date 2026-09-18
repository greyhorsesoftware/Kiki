"""The mouse: what a click actually does, aimed with the geometry the shell reports.

Dragging is not here — `wlrctl` can only click, press and release together, and kiki's rows hand
their drag to the compositor, so a drag needs a pointer that can hold the button down (ydotool).
"""
from harness import snapshot, wait_for

NEEDS = {"shell", "pointer"}
TITLE = "operations with the mouse"


def probe(ctx):
    """One click, to find out whether this compositor delivers virtual-pointer events at all.

    cage accepts the protocol and then logs "wlr_virtual_pointer_v1 cannot be mapped to an output
    device": the events go nowhere. Rather than eight failures that say nothing about kiki, the
    flow steps aside and says why.
    """
    sh, m = ctx.shell, ctx.pointer
    root = ctx.fixture({"one.txt": "1", "two.txt": "2", "three.txt": "3"})
    sh.open("file://" + root)
    sh.call("dismiss")
    if not sh.wait_geometry("row-1"):
        return "the window reports no row to aim at"
    m.click_name("row-1")                   # the middle row: one.txt, three.txt, two.txt sorted
    got = sh.wait_state(lambda st: st.get("selection"), timeout=2)
    if not got:
        return "the compositor does not deliver virtual-pointer events (cage cannot map one to an output)"
    # Landing on a row is not enough: it has to be the row that was aimed at. Relative motion
    # against a multi-output desktop can be a row out, and a flow that clicks the wrong thing
    # reports failures about kiki that are really about the aim.
    want = sh.state().get("selection", [])
    if not any(u.endswith("/three.txt") for u in want):
        return "the pointer does not land where it is aimed here (relative motion, multiple outputs)"
    return None


def run(ctx):
    c, sh, m = ctx.checks, ctx.shell, ctx.pointer
    root = ctx.fixture({"Docs": {"deep.txt": "deep"}, "a.txt": "alpha", "b.txt": "beta"})
    sh.open("file://" + root)
    if not c.check("the rows report a geometry to aim at", sh.wait_geometry("row-0") is not None):
        return

    # Rows are sorted folders first: row-0 is Docs, row-1 a.txt, row-2 b.txt.
    c.check("a click selects the row under the pointer", m.click_name("row-1") and
            sh.wait_state(lambda st: any(u.endswith("/a.txt") for u in st.get("selection", []))) is not None, sh.state().get("selection"))

    c.check("a click on another row moves the selection", m.click_name("row-2") and
            sh.wait_state(lambda st: any(u.endswith("/b.txt") for u in st.get("selection", []))) is not None, sh.state().get("selection"))

    # A double click on a folder opens it, and on the way back out the folder is selected again.
    m.double_click_name("row-0")
    c.check("a double click opens the folder", sh.wait_state(lambda st: st.get("uri", "").endswith("/Docs")) is not None, sh.state().get("uri"))
    sh.call("back")
    c.check("going back lands on the folder we came out of", sh.wait_state(
        lambda st: any(u.endswith("/Docs") for u in st.get("selection", []))) is not None, sh.state().get("selection"))

    # The context menu opens under the pointer and its items are clickable.
    before = snapshot(root)
    m.click_name("row-1", button="right")
    c.check("a right click opens the context menu", sh.wait_state(lambda st: st.get("menuVisible")) is not None, sh.state())
    if c.check("the menu items report a geometry", sh.wait_geometry("menu-New folder") is not None):
        m.click_name("menu-New folder")
        wait_for(lambda: "New folder" in snapshot(root))
        c.tree_changed("clicking New folder makes one", before, snapshot(root), added=["New folder"])
        sh.call("dismiss")

    # Undo from the toast rather than the keyboard.
    sh.select("a.txt")
    before = snapshot(root)
    sh.menu("Move to Trash")
    wait_for(lambda: "a.txt" not in snapshot(root))
    if c.check("the toast offers Undo after a trash", sh.wait_geometry("toast-undo") is not None, sh.state().get("toast")):
        m.click_name("toast-undo")
        wait_for(lambda: "a.txt" in snapshot(root))
        c.same_tree("clicking Undo puts the file back", before, snapshot(root))

    # Permissions are a grid of checkboxes and an Apply button: the mouse path to chmod.
    import os
    sh.select("b.txt")
    sh.call("inspector", "on")
    if not c.check("the inspector opens on request", sh.wait_geometry("tab-permissions") is not None):
        return
    m.click_name("tab-permissions")
    if not c.check("the permissions grid is shown", sh.wait_geometry("perm-owner-1") is not None):
        return
    mode_before = os.stat(root + "/b.txt").st_mode & 0o777
    m.click_name("perm-owner-1")            # owner execute
    m.click_name("perm-apply")
    changed = wait_for(lambda: (os.stat(root + "/b.txt").st_mode & 0o777) != mode_before)
    c.check("ticking a box and clicking Apply changes the mode on disk", changed is not None,
            oct(os.stat(root + "/b.txt").st_mode & 0o777))
    c.check("and it is exactly the bit that was ticked", os.stat(root + "/b.txt").st_mode & 0o777 == mode_before | 0o100,
            oct(os.stat(root + "/b.txt").st_mode & 0o777))
    sh.call("inspector", "off")
