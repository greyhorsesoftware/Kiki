"""What the view shows: hidden files, the filter, and switching views keeps the selection."""
from harness import wait_for

NEEDS = {"shell", "keyboard"}
TITLE = "views, hidden files and the filter"


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    ctx.fixture({"a.txt": "a", "b.txt": "b", "notes.md": "n", ".hidden": "h"})
    root = ctx.root
    sh.open("file://" + root)
    if not c.check("the keymap has the keyboard focus", sh.state().get("keyFocus") is True, sh.state()):
        return

    c.check("dot files are out of the way by default", sh.state().get("count") == 3, sh.state().get("count"))
    sh.keys(("ctrl", "h"))
    c.check("Ctrl+H shows them", sh.wait_state(lambda st: st.get("count") == 4) is not None, sh.state().get("count"))
    sh.keys(("ctrl", "h"))
    c.check("and hides them again", sh.wait_state(lambda st: st.get("count") == 3) is not None, sh.state().get("count"))

    # The filter narrows the listing in place and Escape puts it back.
    sh.call("search", "notes")
    c.check("the filter narrows the listing", sh.wait_state(lambda st: st.get("count") == 1) is not None, sh.state().get("count"))
    sh.call("search", "")
    c.check("clearing it restores the folder", sh.wait_state(lambda st: st.get("count") == 3) is not None)
    # The filter bar keeps the focus while it is open, so the next shortcut would be typed into
    # it: Escape is what closes it and hands the keymap back.
    sh.keys(("Escape",))
    c.check("Escape closes the filter bar", sh.wait_state(lambda st: not st.get("filterOpen") and st.get("keyFocus")) is not None, sh.state())

    # The selection survives a change of view, which is what makes the arrow keys usable.
    sh.select("b.txt")
    sh.keys(("ctrl", "1"))
    c.check("Ctrl+1 switches to icon view", sh.wait_state(lambda st: st.get("view") == "icon") is not None, sh.state().get("view"))
    c.check("the selection survives the switch", any(u.endswith("/b.txt") for u in sh.state().get("selection", [])), sh.state().get("selection"))
    sh.keys(("ctrl", "2"))
    c.check("Ctrl+2 switches back to list", sh.wait_state(lambda st: st.get("view") == "list") is not None)
    c.check("and the selection is still there", any(u.endswith("/b.txt") for u in sh.state().get("selection", [])))
