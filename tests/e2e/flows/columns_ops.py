"""Columns: what the keyboard acts on is the row the key column highlights.

Each column keeps its own highlight — the trail drilled down in grey, the column last clicked in
the accent — and that last one is the choice. The pane's own selection cannot answer for it: the
row may live in a folder the pane is not standing in, and a selection left behind by another view
is not what is on screen. Before this was so, `Del` in Columns trashed the row chosen in List
view while the inspector showed another file; the `Menu` key put up that other file's menu, and
`F2` renamed it after throwing the columns away for a list view to find an editor in.
"""
import urllib.parse
import time
import json
import os
import shutil
import subprocess

from harness import snapshot, wait_for

NEEDS = {"shell", "keyboard"}
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

    # A picture of the rule, for the eye: the key column in the accent, the trail behind it grey.
    if shutil.which("grim"):
        subprocess.run(["grim", os.path.join(os.environ.get("KIKI_E2E_OUT", "/tmp"), "columns-highlight.png")],
                       capture_output=True)
    sh.call("action", "trash")
    c.check("Del there takes that row",
            wait_for(lambda: ("deep.txt" not in snapshot(os.path.join(root, "Projects"))) or None, timeout=30) is not None,
            list(snapshot(os.path.join(root, "Projects"))))
    sh.call("columns", "left")
    st = columns(sh)
    c.check("back in the first column, Projects highlighted", st.get("focusCol") == 0 and (st.get("selected") or [None])[0] == 0, st)

    # ------------------------------------------------------------------ the Menu key, and F2
    # Both went through the pane's selection as everything else used to: the menu was built for
    # whatever another view had left chosen, and F2 renamed that file — after switching the pane
    # to list view to find an editor, which threw the columns away as well.
    root2 = ctx.fixture({"one.txt": "1", "two.txt": "2", "Deep": {"buried.txt": "b"}})
    sh.open("file://" + root2)
    sh.call("setView", "list")
    c.check("one.txt is the pane's own selection", sh.select("one.txt") is not None, sh.state())
    sh.call("setView", "columns")
    for _ in range(3):
        sh.call("columns", "down")                # Deep, one.txt, two.txt
    two = "file://" + os.path.join(root2, "two.txt")
    c.check("two.txt is what the key column highlights",
            sh.wait_state(lambda s: s.get("selection") == [two]) is not None, sh.state().get("selection"))

    sh.keys(("Menu",))
    c.check("the Menu key opens a menu", sh.wait_state(lambda s: s.get("menuVisible")) is not None, sh.state())
    items = json.loads(sh.call("menuItems") or "[]")
    labels = [i["label"] for i in items]
    # The two menus are the same list row for row now (the columns one had fallen behind: no
    # Paste, no New folder, none of the ways of sending). What tells them apart is what is LIVE:
    # the pane's own menu, with nothing of the pane's selected in this view, greys everything
    # that needs a file; the highlighted row's has them all ready.
    live = {i["label"] for i in items if i.get("enabled")}
    c.check("it is the highlighted row's menu, not the pane's",
            {"Open", "Rename", "Copy", "Move to Trash"} <= live, (labels, sorted(live)))
    c.check("and it is the whole menu: Paste, New folder and the ways of sending are on it",
            "Paste" in labels and "New folder" in labels and "Extract to…" in labels, labels)
    sh.keys(("Escape",))
    c.check("Escape closes it", sh.wait_state(lambda s: not s.get("menuVisible")) is not None, sh.state())

    sh.menu("Move to Trash")
    c.check("and its actions take the highlighted row",
            wait_for(lambda: ("two.txt" not in snapshot(root2)) or None, timeout=30) is not None, list(snapshot(root2)))
    c.check("…leaving the row list view had chosen alone", "one.txt" in snapshot(root2), list(snapshot(root2)))

    # F2: the editor opens on the row the key column highlights, in the column it lives in, and
    # the view stays where it is. Wait for the row that went to leave the listing first, or the
    # arrows below are counted against rows that are not there any more.
    sh.wait_state(lambda s: s.get("count") == 2)
    sh.call("columns", "down")                    # Deep
    sh.call("columns", "right")                   # into it, onto buried.txt
    inner = os.path.join(root2, "Deep")
    deep2 = "file://" + os.path.join(inner, "buried.txt")
    c.check("a row in the deeper column is highlighted",
            sh.wait_state(lambda s: s.get("selection") == [deep2]) is not None, sh.state().get("selection"))

    # And with the pane's own selection empty — which is how Columns is used, since it never sets
    # one — a menu built from it was every item greyed: the Menu key did nothing you could use.
    sh.call("dismiss")
    c.check("the row on screen is still what is chosen", sh.state().get("selection") == [deep2], sh.state().get("selection"))
    sh.keys(("Menu",))
    sh.wait_state(lambda s: s.get("menuVisible"))
    live = {i["label"]: i["enabled"] for i in json.loads(sh.call("menuItems") or "[]")}
    c.check("the menu is live for the row on screen, with nothing in the pane's selection",
            bool(live.get("Open") and live.get("Copy") and live.get("Move to Trash")), live)
    sh.keys(("Escape",))
    sh.wait_state(lambda s: not s.get("menuVisible"))

    sh.keys(("F2",))
    c.check("F2 opens the inline editor", sh.wait_state(lambda s: s.get("renaming", -1) >= 0) is not None, sh.state())
    c.check("…without leaving Columns to find one", sh.state().get("view") == "columns", sh.state().get("view"))
    sh.type("renamed")          # the editor preselects the stem, so only the stem is retyped
    sh.keys(("Return",))
    wait_for(lambda: ("renamed.txt" in snapshot(inner)) or None, timeout=30)
    c.check("F2 renames the highlighted row, in the folder it lives in",
            "renamed.txt" in snapshot(inner) and "buried.txt" not in snapshot(inner), list(snapshot(inner)))
    c.check("…and nothing in the folder the pane is standing in", "one.txt" in snapshot(root2), list(snapshot(root2)))
    sh.call("dismiss")

    # ------------------------------------------------------------------ a new folder, and Rename
    # Ctrl+Shift+N made the folder in the folder the PANE was on and then switched the view to
    # list to find an editor to name it in — the columns thrown away, and the folder made behind
    # the one on screen. It goes into the key column's folder now, and is named there.
    sh.keys(("ctrl", "shift", "n"))
    made = wait_for(lambda: ("New folder" in snapshot(inner)) or None, timeout=30)
    c.check("Ctrl+Shift+N makes the folder in the key column's folder", made is not None, list(snapshot(inner)))
    c.check("…and not in the folder the pane is standing in", "New folder" not in snapshot(root2), list(snapshot(root2)))
    c.check("the new folder opens its editor",
            sh.wait_state(lambda s: s.get("renaming", -1) >= 0) is not None, sh.state())
    c.check("…without leaving Columns to find one", sh.state().get("view") == "columns", sh.state().get("view"))
    c.check("and it is what the key column highlights",
            sh.wait_state(lambda s: s.get("selection") == ["file://" + os.path.join(inner, "New%20folder")]) is not None,
            sh.state().get("selection"))
    sh.type("named")            # the whole name is preselected, so typing replaces it
    sh.keys(("Return",))
    wait_for(lambda: ("named" in snapshot(inner)) or None, timeout=30)
    c.check("typing a name and Enter renames it on disk",
            "named" in snapshot(inner) and "New folder" not in snapshot(inner), list(snapshot(inner)))
    c.check("…and Columns is still what is showing", sh.state().get("view") == "columns", sh.state().get("view"))

    # The row menu had no Rename at all — columns had no editor for one to open. It acts on the
    # row the menu belongs to, in the column that row lives in. A rename leaves the column with
    # nothing highlighted (the row it was on has gone), so choose one first: the folder just made.
    sh.call("dismiss")
    sh.call("columns", "down")
    named = "file://" + os.path.join(inner, "named")
    c.check("the folder just made is what the key column highlights",
            sh.wait_state(lambda s: s.get("selection") == [named]) is not None, sh.state().get("selection"))
    sh.keys(("Menu",))
    sh.wait_state(lambda s: s.get("menuVisible"))
    live = {i["label"]: i["enabled"] for i in json.loads(sh.call("menuItems") or "[]")}
    c.check("the row's menu offers a live Rename", live.get("Rename") is True, live)
    sh.keys(("Escape",))
    sh.wait_state(lambda s: not s.get("menuVisible"))
    sh.menu("Rename")
    c.check("choosing it opens the editor on the highlighted row",
            sh.wait_state(lambda s: s.get("renaming", -1) >= 0) is not None, sh.state())
    c.check("…and Columns is still what is showing", sh.state().get("view") == "columns", sh.state().get("view"))
    sh.type("again")
    sh.keys(("Return",))
    wait_for(lambda: ("again" in snapshot(inner)) or None, timeout=30)
    c.check("the menu's Rename renames the row it belongs to",
            "again" in snapshot(inner) and "named" not in snapshot(inner), list(snapshot(inner)))
    sh.call("dismiss")

    # ------------------------------------------------------------------ what KIND is chosen
    # Asked of the columns too. A folder highlighted in the key column with a file still the
    # pane's own selection: project mode opens on the folder; judged by the pane's row it opened
    # on the wrong place, or not at all. Last in the flow, because the editor and the agent it
    # starts are windows of their own and take the compositor's keyboard with them — no key sent
    # after this point reaches kiki.
    sh.call("setView", "list")
    c.check("a file is the pane's own selection", sh.select("one.txt") is not None, sh.state())
    sh.call("setView", "columns")
    sh.call("columns", "down")                    # Deep, the first row
    st = columns(sh)
    c.check("a folder is highlighted in the key column",
            st.get("focusCol") == 0 and (st.get("selected") or [None])[0] == 0, st)
    sh.call("action", "project")
    project = wait_for(lambda: (lambda p: p if p.get("active") else None)(json.loads(sh.call("projectState") or "{}")), timeout=10)
    c.check("project mode opens on the highlighted folder",
            project is not None and project.get("root") == "file://" + inner, project or sh.call("projectState"))
    sh.call("project", "leave", "")
    # The tree takes the keyboard while project mode is up and hands it to nobody on the way out:
    # every shortcut was dead afterwards, Escape included, until something was clicked.
    c.check("the keyboard comes back when project mode ends",
            sh.wait_state(lambda s: s.get("keyFocus") is True) is not None, sh.state())
    # And a key really does something: the favorites panel, asked for by its chord, and put back.
    # (It was Side by Side's Ctrl+4, which does nothing now unless a server is open.)
    was = sh.state().get("sidebar")
    sh.keys(("ctrl", "shift", "b"))
    c.check("and a shortcut pressed then is heard", sh.wait_state(lambda s: s.get("sidebar") is (not was)) is not None, sh.state())
    sh.keys(("ctrl", "shift", "b"))
    sh.wait_state(lambda s: s.get("sidebar") is was)

    # ------------------------------------------------------------------ inside what is highlighted
    # A folder is highlighted and the accent is on the column it is IN. Ctrl+Shift+N used to make
    # the folder there, beside it; the owner's rule (2026-09-21) is that it lands inside it.
    sh.call("setView", "columns")
    sh.call("columns", "down"); sh.call("columns", "up")          # a row of the key column, highlighted
    chosen = (sh.state().get("selection") or [""])[0]
    target = urllib.parse.unquote(chosen[len("file://"):])
    if c.check("a folder is highlighted in the key column", os.path.isdir(target), (chosen, columns(sh))):
        inside, beside = set(os.listdir(target)), set(os.listdir(os.path.dirname(target)))
        sh.keys(("ctrl", "shift", "n"))
        made = wait_for(lambda: (set(os.listdir(target)) - inside) or None, timeout=30)
        c.check("Ctrl+Shift+N makes the folder inside the highlighted folder", made == {"New folder"}, (made, os.listdir(target)))
        c.check("…and nothing beside it", set(os.listdir(os.path.dirname(target))) == beside, os.listdir(os.path.dirname(target)))
        c.check("named where it landed, without leaving Columns",
                sh.wait_state(lambda s: s.get("renaming", -1) >= 0 and s.get("view") == "columns") is not None, sh.state())
        sh.keys(("Escape",))
    sh.call("setView", "list")
