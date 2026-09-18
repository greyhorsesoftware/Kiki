"""Operations that ask first: nothing destructive happens until the dialog is answered."""
from harness import snapshot, wait_for

NEEDS = {"shell", "keyboard"}
TITLE = "delete for good, with the confirmation"


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    root = ctx.fixture({"a.txt": "alpha", "b.txt": "beta"})
    sh.open("file://" + root)
    if not c.check("the keymap has the keyboard focus", sh.state().get("keyFocus") is True, sh.state()):
        return

    # Shift+Del asks, and Escape means no: the file is still there afterwards.
    sh.select("a.txt")
    sh.keys(("shift", "Delete"))
    c.check("Shift+Del asks before deleting", sh.wait_state(lambda st: st.get("dialogs", {}).get("confirm")) is not None, sh.state())
    sh.keys(("Escape",))
    c.check("Escape closes the question", sh.wait_state(lambda st: not st.get("dialogs", {}).get("confirm")) is not None)
    c.check("and the file is untouched", "a.txt" in snapshot(root))

    # Answering yes deletes it for good: gone from the folder and never in the trash.
    sh.select("a.txt")
    sh.keys(("shift", "Delete"))
    sh.wait_state(lambda st: st.get("dialogs", {}).get("confirm"))
    sh.keys(("Return",))
    wait_for(lambda: "a.txt" not in snapshot(root))
    after = snapshot(root)
    c.check("Enter deletes it for good", "a.txt" not in after, list(after))
    c.check("the other file is left alone", after.get("b.txt") == b"beta")
    infos = ctx.daemon.ok("TrashInfo")["items"]
    c.check("nothing went to the trash", not any(i["path"].endswith("/a.txt") for i in infos))
