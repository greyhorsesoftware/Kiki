"""Columns: what the keyboard acts on is the row the key column highlights.

Each column keeps its own highlight — the trail drilled down in grey, the column last clicked in
the accent — and that last one is the choice. The pane's own selection cannot answer for it: the
row may live in a folder the pane is not standing in, and a selection left behind by another view
is not what is on screen. Before this was so, `Del` in Columns trashed the row chosen in List
view while the inspector showed another file.
"""
import json
import os

from harness import snapshot, wait_for

NEEDS = {"shell"}
TITLE = "columns: the key column is the selection"


def columns(sh):
    try:
        return json.loads(sh.call("columns", "state") or "{}")
    except json.JSONDecodeError:
        return {}


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    root = ctx.fixture({"a.txt": "a", "b.txt": "b", "Projects": {"deep.txt": "d"}})
    sh.open("file://" + root)
    sh.call("setView", "list")
    if not c.check("a.txt is chosen in list view", sh.select("a.txt") is not None, sh.state()):
        return
    sh.call("setView", "columns")

    # Down twice: past Projects, onto a.txt... then once more, onto b.txt.
    for _ in range(3):
        sh.call("columns", "down")
    st = columns(sh)
    c.check("the highlight is on the last row", st.get("selected") == [2], st)
    c.check("and that is what the window says is chosen",
            sh.state().get("selection") == ["file://" + os.path.join(root, "b.txt")], sh.state().get("selection"))

    # Del takes the highlighted row — not a.txt, which list view had left selected.
    sh.call("action", "trash")
    gone = wait_for(lambda: ("b.txt" not in snapshot(root)) or None, timeout=30)
    after = snapshot(root)
    c.check("Del trashes the highlighted row", gone is not None and "b.txt" not in after, list(after))
    c.check("…and leaves the row another view had chosen alone", "a.txt" in after, list(after))

    # A row in a deeper column: chosen where it lives, and the pane is not standing there.
    sh.call("columns", "up")
    sh.call("columns", "up")                      # back to Projects
    sh.call("columns", "right")                   # into it, onto deep.txt
    deep = "file://" + os.path.join(root, "Projects", "deep.txt")
    chosen = sh.wait_state(lambda s: s.get("selection") == [deep])
    c.check("a row in a deeper column is chosen where it lives", chosen is not None, sh.state().get("selection"))
    c.check("…while the pane stands in the folder above", sh.state().get("uri") == "file://" + root, sh.state().get("uri"))
    sh.call("action", "trash")
    c.check("Del there takes that row",
            wait_for(lambda: ("deep.txt" not in snapshot(os.path.join(root, "Projects"))) or None, timeout=30) is not None,
            list(snapshot(os.path.join(root, "Projects"))))
    sh.call("setView", "list")
