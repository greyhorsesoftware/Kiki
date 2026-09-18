"""The trash as a folder: what is in it, restoring from it, emptying it."""
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
    c.check("the file that was never trashed is still there", "stay.txt" in snapshot(root))
