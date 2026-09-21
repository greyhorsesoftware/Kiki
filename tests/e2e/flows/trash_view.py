"""The trash as a folder: what is in it, restoring from it, emptying it — and that nothing goes
INTO it. Paste, New folder and a drop are all refused there: what is in the trash is on its way
out. The Trash in the SIDEBAR is a different target with the same URI, and a drop on that one
trashes what is dropped.
"""
import json
import os

from harness import snapshot, wait_for

NEEDS = {"shell", "keyboard"}
TITLE = "the trash view"


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    root = ctx.fixture({"gone.txt": "gone", "stay.txt": "stay"})
    sh.open("file://" + root)
    if not c.check("the keymap has the keyboard focus", sh.state().get("keyFocus") is True, sh.state()):
        return

    sh.select("gone.txt")
    sh.keys(("Delete",))
    wait_for(lambda: "gone.txt" not in snapshot(root))
    c.check("Del puts it in the trash", "gone.txt" not in snapshot(root))

    sh.open("trash:///")
    listed = sh.wait_state(lambda st: st.get("count", 0) >= 1)
    c.check("the trash view lists it", listed is not None, sh.state())

    # Enter on a trashed row is Restore: it goes back where it came from.
    sh.select("gone.txt")
    sh.keys(("Return",))
    wait_for(lambda: "gone.txt" in snapshot(root))
    c.check("Enter restores it to the folder it came from", snapshot(root).get("gone.txt") == b"gone", list(snapshot(root)))

    # Empty Trash asks first, then leaves nothing behind.
    sh.open("file://" + root)
    sh.select("gone.txt")
    sh.keys(("Delete",))
    wait_for(lambda: "gone.txt" not in snapshot(root))
    sh.open("trash:///")
    sh.menu("Empty Trash")
    c.check("Empty Trash asks first", sh.wait_state(lambda st: st.get("dialogs", {}).get("confirm")) is not None, sh.state())
    sh.keys(("Return",))
    emptied = wait_for(lambda: not d.ok("TrashInfo")["items"])
    c.check("and then the trash is empty", emptied is not None, d.ok("TrashInfo")["items"])
    c.check("and the view empties with it", sh.wait_state(lambda st: st.get("count") == 0) is not None, sh.state())
    c.check("the file that was never trashed is still there", "stay.txt" in snapshot(root))

    # ------------------------------------------------------------ nothing goes into the trash
    other = ctx.fixture({"take.txt": "t", "into": {}})
    into = os.path.join(other, "into")
    sh.open("file://" + other)
    c.check("a file to copy", sh.select("take.txt") is not None, sh.state())
    sh.keys(("ctrl", "c"))
    c.check("it is on the clipboard", wait_for(lambda: sh.state().get("clipboard")) is not None, sh.state())

    sh.open("trash:///")
    sh.wait_state(lambda st: st.get("done") is True)
    sh.keys(("Menu",))
    c.check("the trash view has a menu of its own", sh.wait_state(lambda st: st.get("menuVisible")) is not None, sh.state())
    labels = [i["label"] for i in json.loads(sh.call("menuItems") or "[]")]
    c.check("and it offers neither Paste nor New folder",
            bool(labels) and "Paste" not in labels and "New folder" not in labels, labels)
    sh.keys(("Escape",))
    sh.wait_state(lambda st: not st.get("menuVisible"))

    sh.keys(("ctrl", "v"))                        # both refused: the trash is not a folder to
    sh.keys(("ctrl", "shift", "n"))               # put things in
    # A drop on the trash VIEW's own pane — not the sidebar's Trash, which trashes what it is
    # given (`drag_between_panes`). The pane's folder is the trash, and nothing goes into it.
    r = json.loads(sh.call("dropOn", "left", "file://" + os.path.join(other, "take.txt"), "trash:///", "") or "{}")
    c.check("a drop onto the trash view's own pane is refused",
            r.get("accepted") is False and r.get("action") == "none", r)

    # Proving a negative needs something positive to follow it: the same Ctrl+V in a folder that
    # does take one works, so the keys above reached the window and did nothing, rather than not
    # having arrived yet.
    sh.open("file://" + into)
    sh.keys(("ctrl", "v"))
    wait_for(lambda: ("take.txt" in snapshot(into)) or None, timeout=30)
    c.check("the same Ctrl+V in a folder that takes one pastes", "take.txt" in snapshot(into), list(snapshot(into)))

    sh.open("trash:///")
    sh.wait_state(lambda st: st.get("done") is True)
    c.check("and the trash is as empty as it was — no paste, no new folder, no drop",
            sh.state().get("count") == 0 and not d.ok("TrashInfo")["items"],
            (sh.state().get("count"), d.ok("TrashInfo")["items"]))
    c.check("…with no editor left open on a folder that was never made",
            sh.state().get("renaming", -1) < 0, sh.state())
