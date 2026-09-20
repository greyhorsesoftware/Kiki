"""Side by side: the toolbar's own toggle, a divider that drags, and a path over each pane."""
import json, os

NEEDS = {"shell", "keyboard"}
TITLE = "side by side: toggle, divider, paths"


def sbs(sh, action="state"):
    try:
        return json.loads(sh.call("sideBySide", action) or "{}")
    except json.JSONDecodeError:
        return {}


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    ctx.fixture({"left": {"a.txt": "a"}, "right": {"b.txt": "b"}})
    left = "file://" + os.path.join(ctx.root, "left")
    sh.open(left)
    if not c.check("the keymap has the keyboard focus", sh.state().get("keyFocus") is True, sh.state()):
        return

    # A folder with a remembered view, so there is something for side by side to ignore.
    from harness import wait_for
    views = os.path.join(os.environ["KIKI_CONFIG_DIR"], "views.toml")
    sh.keys(("ctrl", "1"))
    c.check("the folder is remembered as Icon view", sh.wait_state(lambda st: st.get("view") == "icon") is not None
            and wait_for(lambda: (os.path.exists(views) and "icon" in open(views).read()) or None, what="icon in views.toml") is not None)
    remembered = open(views).read()

    sh.keys(("ctrl", "4"))
    c.check("Ctrl+4 turns side by side on", sh.wait_state(lambda st: st.get("split") is True) is not None, sh.state().get("split"))
    st = sbs(sh)
    total = st.get("total", 0)
    c.check("the panes start half and half", abs(st.get("left", 0) - (total - 1) / 2) <= 1, st)

    # The strip that used to sit above the panes is gone; the way into a mirror run is in the
    # middle of the toolbar.
    c.check("there is no bar above the two panes", not sh.geometry("mirror-bar"), sh.geometry("mirror-bar"))
    mb, tb = sh.geometry("toolbar-mirror"), sh.geometry("toolbar")
    c.check("the mirror button is in the toolbar", bool(mb) and mb.get("w", 0) > 0, mb)
    dv = sh.geometry("pane-divider")
    c.check("standing over the line between the panes, not over the middle of the window",
            mb and dv and abs((mb["x"] + mb["w"] / 2) - (dv["x"] + dv["w"] / 2)) <= 2, (mb, dv, tb))

    # The divider: a ratio that holds, a minimum either side, one write when the drag ends.
    want = int(total * 0.7)
    st = sbs(sh, "drag %d" % want)
    c.check("dragging the line gives the left pane that room", abs(st.get("left", 0) - want) <= 2 and st.get("dragging") is True, st)
    mb, dv = sh.geometry("toolbar-mirror"), sh.geometry("pane-divider")
    c.check("the mirror button goes with the line", mb and dv and abs((mb["x"] + mb["w"] / 2) - (dv["x"] + dv["w"] / 2)) <= 2, (mb, dv))
    lg, rg = sh.geometry("pane-left"), sh.geometry("pane-right")
    c.check("and the panes on screen follow it", lg and rg and abs(lg["w"] - st["left"]) <= 1 and abs(lg["w"] + rg["w"] + 1 - total) <= 1, (lg, rg))
    settings = os.path.join(os.environ["KIKI_CONFIG_DIR"], "settings.toml")
    before = open(settings).read() if os.path.exists(settings) else ""
    c.check("nothing is written while the drag is still going", "sideBySideRatio" not in before, before)
    st = sbs(sh, "end")
    c.check("letting go keeps the ratio", abs(st.get("ratio", 0) - 0.7) < 0.02 and st.get("dragging") is False, st)
    c.check("and writes it once, as a window setting",
            wait_for(lambda: ("sideBySideRatio" in open(settings).read()) or None, what="sideBySideRatio in settings.toml") is not None)

    st = sbs(sh, "drag 5")
    c.check("the left pane cannot be dragged shut", st.get("left", 0) >= min(st.get("min", 280), (total - 1) // 2) - 1, st)
    st = sbs(sh, "drag %d" % (total - 5))
    c.check("nor the right one", total - 1 - st.get("left", 0) >= min(st.get("min", 280), (total - 1) // 2) - 1, st)
    st = sbs(sh, "reset")
    c.check("a double click is half and half again", abs(st.get("left", 0) - (total - 1) / 2) <= 1, st)

    # Each pane carries its own path; the one in the title bar steps aside while they are up.
    st = sbs(sh)
    c.check("the title bar's path is out of the way", st.get("titlePathShown") is False, st)
    c.check("the left pane shows its own path", st.get("leftPath") == left, st)
    c.check("and so does the right one", bool(st.get("rightPath")) and st.get("rightPath") == st.get("rightUri"), st)
    sh.call("focusPane", "right")
    c.check("moving the focus changes neither", sbs(sh).get("leftPath") == left, sbs(sh))

    # The view menu ticks the FOCUSED pane's view, split or not. (It ticked nothing when split.)
    import json as _json
    def ticks():
        sh.call("viewMenu")
        items = wait_for(lambda: _json.loads(sh.call("menuItems") or "[]") or None, what="view menu open") or []
        sh.call("viewMenu")
        return [i["label"] for i in items if i["checked"] and i["label"] != "Show hidden files"]
    sh.call("focusPane", "left"); sh.keys(("ctrl", "1"))
    sh.call("focusPane", "right"); sh.keys(("ctrl", "2"))
    wait_for(lambda: sbs(sh).get("leftView") == "icon" and sbs(sh).get("rightView") == "list" or None, what="one pane in icon, one in list")
    c.check("the view menu ticks the right pane's view while it has the focus", ticks() == ["List"], ticks())
    sh.call("focusPane", "left")
    c.check("and the left pane's when the focus moves", ticks() == ["Icon"], ticks())
    sh.keys(("ctrl", "2"))

    # Tab changes pane. (A property called `otherPane` once shadowed the function of that name:
    # Tab and copy-to-the-other-pane both threw, and nothing drove either.)
    sh.call("focusPane", "left")
    sh.keys(("Tab",))
    c.check("Tab moves the focus to the other pane", (wait_for(lambda: sbs(sh).get("focused") == "right" or None, what="focus right") or False) is True, sbs(sh))
    sh.keys(("Tab",))
    c.check("and back", (wait_for(lambda: sbs(sh).get("focused") == "left" or None, what="focus left") or False) is True, sbs(sh))

    # The overlays that make any press focus a pane cover the pane's VIEW — everything under its
    # header, which takes a click for itself — and must not have pushed the headers down.
    for side in ("left", "right"):
        pg, fg, hg = sh.geometry("pane-" + side), sh.geometry("pane-focus-" + side), sh.geometry("pane-header-" + side)
        c.check("the %s pane's focus catcher covers the view under the header" % side,
                pg and fg and hg and abs(pg["x"] - fg["x"]) <= 1 and abs(pg["w"] - fg["w"]) <= 1
                and abs((hg["y"] + hg["h"]) - fg["y"]) <= 1 and abs((pg["y"] + pg["h"]) - (fg["y"] + fg["h"])) <= 1, (pg, fg, hg))
        c.check("and the %s header is still at the top of it" % side, pg and hg and abs(pg["y"] - hg["y"]) <= 1, (pg, hg))

    # Each pane has a view of its own, the view keys change the focused one, and none of it is
    # anybody's preference.
    st = sbs(sh)
    c.check("both panes start in List, whatever the folder remembers", st.get("leftView") == "list" and st.get("rightView") == "list", st)
    c.check("and view memory is off", st.get("remembering") is False, st)
    sh.call("focusPane", "right")
    sh.keys(("ctrl", "1"))
    st = wait_for(lambda: (lambda x: x if x.get("rightView") == "icon" else None)(sbs(sh)), what="right pane in icon view") or sbs(sh)
    c.check("Ctrl+1 changes the focused pane's view", st.get("rightView") == "icon", st)
    c.check("and only that one", st.get("leftView") == "list", st)
    c.check("without leaving side by side", st.get("split") is True, st)
    right_uri = st.get("rightUri")

    # Turning it off keeps the pane you were in.
    sh.keys(("ctrl", "4"))
    c.check("Ctrl+4 with the focus on the right is one pane", sh.wait_state(lambda st: st.get("split") is False) is not None)
    c.check("and it is the right pane's folder that stays", sh.state().get("uri") == right_uri, (sh.state().get("uri"), right_uri))
    sh.keys(("ctrl", "4"))
    sh.wait_state(lambda st: st.get("split") is True)
    st = sbs(sh)
    c.check("turning it on again puts both folders back where they were", st.get("leftUri") == left and st.get("rightUri") == right_uri, st)

    sh.call("focusPane", "left")
    sh.keys(("ctrl", "4"))
    c.check("Ctrl+4 again is one pane", sh.wait_state(lambda st: st.get("split") is False) is not None)
    c.check("the left folder this time", sh.state().get("uri") == left, sh.state().get("uri"))
    c.check("opening the way it is remembered, not the way it was split", sh.wait_state(lambda st: st.get("view") == "icon") is not None, sh.state().get("view"))
    import time; time.sleep(0.5)
    c.check("and views.toml is exactly as it was before side by side", open(views).read() == remembered, open(views).read())
    c.check("and the title bar has its path back", sbs(sh).get("titlePathShown") is True, sbs(sh))
