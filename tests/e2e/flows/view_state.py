"""What the view shows: hidden files, the filter, switching views keeps the selection, and the
list's column widths outlive the folder they were set in."""
import json, os
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

    # A list column is resized by dragging the edge it begins at; `listColumn` is that drag. The
    # width belongs to list view rather than to the folder, so it holds wherever the list goes.
    settings = os.path.join(os.environ["KIKI_CONFIG_DIR"], "settings.toml")
    before = sh.state().get("listColumns", {})
    c.check("the list says what its columns are drawn at", before.get("mtime") == 160, before)
    w = json.loads(sh.call("listColumn", "mtime", 240) or "{}")
    c.check("a resize widens the column", w.get("mtime") == 240, w)
    c.check("and Name gives up exactly what it took", w.get("name") == before.get("name", 0) - 80, (before, w))
    c.check("shell state carries the widths", sh.state().get("listColumns", {}).get("mtime") == 240, sh.state().get("listColumns"))
    c.check("the width is written to settings.toml",
            wait_for(lambda: ("listColumnWidths" in open(settings).read()) or None, what="listColumnWidths in settings.toml") is not None)
    c.check("under the view section, by role", "[view.listColumnWidths]" in open(settings).read() and "mtime = 240" in open(settings).read(),
            open(settings).read())

    # Another folder, opened from scratch, and then this one again: one set of widths, not one
    # per folder. Reopening rebuilds the listing and the rows with it.
    ctx.fixture({"x.txt": "x"})
    sh.open("file://" + ctx.root)
    c.check("a different folder is drawn with the same widths", sh.state().get("listColumns", {}).get("mtime") == 240, sh.state().get("listColumns"))
    sh.open("file://" + root)
    c.check("and so is this one, listed again", sh.state().get("listColumns", {}).get("mtime") == 240, sh.state().get("listColumns"))

    # A double click on the edge puts the column back, which is also how this flow leaves the
    # run's settings as it found them.
    w = json.loads(sh.call("listColumn", "mtime", "reset") or "{}")
    c.check("resetting puts the column back to its default", w.get("mtime") == 160, w)
