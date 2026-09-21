"""Side by side: the toolbar's own toggle, a divider that drags, a path over each pane — and
what the layout is actually for: moving files between the two folders it shows.

The transfers are plan 07's: a copy and a move across the panes are ordinary jobs, so Ctrl+Z
takes each of them back (a copy's inverse deletes what landed, a move's moves it home), and
collapsing the layout and opening it again finds both panes as they were, selections included.

The last three want a server, because pairing is what a location is: `openLocation` puts its
local path on the left and its server on the right, `Ctrl+4` on a plain local folder puts that
location back on the right, and a remote URI typed into the breadcrumb finds the location it
belongs to and opens the pair. Without `sshd` those three are skipped by name and the rest runs.
"""
import getpass, json, os

from harness import snapshot, wait_for
from servers import Servers, add_location, sshd_bin

NEEDS = {"shell", "keyboard"}
TITLE = "side by side: toggle, divider, paths, transfers"


def sbs(sh, action="state"):
    try:
        return json.loads(sh.call("sideBySide", action) or "{}")
    except json.JSONDecodeError:
        return {}


def settled(d):
    """Wait until no job is still going. A copy or a move reaches the journal when it ENDS, so a
    Ctrl+Z sent the moment the file appears can undo whatever came before it instead."""
    return wait_for(lambda: all(j.get("state") not in ("queued", "running") for j in d.ok("Jobs")["jobs"]) or None, timeout=60)


def split_state(sh, on=True):
    """The layout's state once it has settled into `on`, or whatever it settled on instead."""
    return wait_for(lambda: (lambda st: st if st.get("split") is on else None)(sbs(sh)), what="side by side %s" % on) or sbs(sh)


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    ctx.fixture({"left": {"a.txt": "a"}, "right": {"b.txt": "b"}})
    left = "file://" + os.path.join(ctx.root, "left")
    sh.open(left)
    if not c.check("the keymap has the keyboard focus", sh.state().get("keyFocus") is True, sh.state()):
        return

    # A folder with a remembered view, so there is something for side by side to ignore.
    views = os.path.join(os.environ["KIKI_CONFIG_DIR"], "views.toml")
    sh.keys(("ctrl", "1"))
    c.check("the folder is remembered as Icon view", sh.wait_state(lambda st: st.get("view") == "icon") is not None
            and wait_for(lambda: (os.path.exists(views) and "icon" in open(views).read()) or None, what="icon in views.toml") is not None)
    remembered = open(views).read()

    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    c.check("Ctrl+4 turns side by side on", sh.wait_state(lambda st: st.get("split") is True) is not None, sh.state().get("split"))
    st = sbs(sh)
    # Plan 24: the right pane starts at the last location opened — and at home when there has
    # been none, which is where this window stands: nothing has opened a location yet.
    c.check("with no location behind it, the other pane starts at home", st.get("rightUri") == "file://" + os.environ["HOME"], st)
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
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    c.check("Ctrl+4 with the focus on the right is one pane", sh.wait_state(lambda st: st.get("split") is False) is not None)
    c.check("and it is the right pane's folder that stays", sh.state().get("uri") == right_uri, (sh.state().get("uri"), right_uri))
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    sh.wait_state(lambda st: st.get("split") is True)
    st = sbs(sh)
    c.check("turning it on again puts both folders back where they were", st.get("leftUri") == left and st.get("rightUri") == right_uri, st)

    sh.call("focusPane", "left")
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    c.check("Ctrl+4 again is one pane", sh.wait_state(lambda st: st.get("split") is False) is not None)
    c.check("the left folder this time", sh.state().get("uri") == left, sh.state().get("uri"))
    c.check("opening the way it is remembered, not the way it was split", sh.wait_state(lambda st: st.get("view") == "icon") is not None, sh.state().get("view"))
    import time; time.sleep(0.5)
    c.check("and views.toml is exactly as it was before side by side", open(views).read() == remembered, open(views).read())
    c.check("and the title bar has its path back", sbs(sh).get("titlePathShown") is True, sbs(sh))

    # ------------------------------------------------------------- what the layout is for
    # Plan 07: a copy and a move between the panes are jobs like any other, so both are in the
    # undo journal — a copy's inverse deletes what landed, a move's moves it home. Two local
    # folders, one in each pane; no server needed.
    root = ctx.fixture({"from": {"one.txt": "one", "two.txt": "two"}, "to": {"here.txt": "here"}})
    frm, to = os.path.join(root, "from"), os.path.join(root, "to")
    src, dst = "file://" + frm, "file://" + to
    # `to` is remembered as Icon view. Collapsing back onto it has to rebuild its view for the
    # preference, which is where a selection is likeliest to be dropped on the way.
    sh.open(dst)
    sh.keys(("ctrl", "1"))
    c.check("the folder in the other pane is remembered as Icon view",
            sh.wait_state(lambda s: s.get("view") == "icon") is not None, sh.state().get("view"))
    sh.open(src)
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    if not c.check("side by side, a folder in each pane", split_state(sh).get("split") is True, sbs(sh)):
        return
    sh.call("focusPane", "right")
    sh.open(dst)
    sh.call("focusPane", "left")
    started = snapshot(root)

    if not c.check("the file to copy across is selected", sh.select("one.txt") is not None, sh.state()):
        return
    sh.keys(("ctrl", "c"))            # Super+C, as the compositor delivers it (see ops_keyboard)
    # That one file, and not whatever an earlier flow left on the clipboard.
    c.check("the clipboard holds it, to copy and not to cut",
            sh.wait_state(lambda s: s.get("clipboard") == [src + "/one.txt"] and s.get("clipboardCut") is False) is not None,
            (sh.state().get("clipboard"), sh.state().get("clipboardCut")))
    sh.keys(("Tab",))
    c.check("Tab moves to the pane it is going into",
            wait_for(lambda: sbs(sh).get("focused") == "right" or None, what="focus right") is not None, sbs(sh))
    sh.keys(("ctrl", "v"))
    landed = wait_for(lambda: os.path.exists(os.path.join(to, "one.txt")) or None, timeout=60)
    c.check("a copy across the panes lands in the other pane's folder", landed is not None, os.listdir(to))
    c.check("and the original stays where it was", os.path.exists(os.path.join(frm, "one.txt")), os.listdir(frm))
    settled(d)
    sh.keys(("ctrl", "z"))
    c.check("Ctrl+Z takes the copy off the destination",
            wait_for(lambda: (not os.path.exists(os.path.join(to, "one.txt"))) or None, timeout=60) is not None, os.listdir(to))
    settled(d)
    c.same_tree("and leaves both folders as they were", started, snapshot(root))

    # F6 — "Move across" — is the move; it needs no clipboard and no keys held.
    sh.call("focusPane", "left")
    if not c.check("the file to move across is selected", sh.select("two.txt") is not None, sh.state()):
        return
    sh.keys(("F6",))
    moved = wait_for(lambda: (os.path.exists(os.path.join(to, "two.txt")) and not os.path.exists(os.path.join(frm, "two.txt"))) or None, timeout=60)
    c.check("F6 moves the selection into the other pane's folder", moved is not None, (os.listdir(frm), os.listdir(to)))
    settled(d)
    sh.keys(("ctrl", "z"))
    back = wait_for(lambda: os.path.exists(os.path.join(frm, "two.txt")) or None, timeout=60)
    c.check("Ctrl+Z moves it back", back is not None, (os.listdir(frm), os.listdir(to)))
    settled(d)
    c.same_tree("and both trees are as they started", started, snapshot(root))

    # Collapsing and opening the layout again finds both panes as they were — the same two Pane
    # objects, so a selection comes back with its folder rather than being re-made from a URI.
    sh.call("focusPane", "left")
    sh.select("one.txt")
    sh.call("focusPane", "right")
    sh.select("here.txt")
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    c.check("collapsing from the right keeps that pane's folder and its selection",
            sh.wait_state(lambda s: s.get("split") is False and s.get("uri") == dst and s.get("selection") == [dst + "/here.txt"]) is not None, sh.state())
    c.check("and draws it the way that folder is remembered, view rebuilt and selection still there",
            sh.wait_state(lambda s: s.get("view") == "icon" and s.get("selection") == [dst + "/here.txt"]) is not None,
            (sh.state().get("view"), sh.state().get("selection")))
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    st = split_state(sh)
    c.check("opening it again puts both folders back", st.get("leftUri") == src and st.get("rightUri") == dst, st)
    sh.call("focusPane", "left")
    c.check("the left pane's selection is what it was",
            sh.wait_state(lambda s: s.get("selection") == [src + "/one.txt"]) is not None, sh.state().get("selection"))
    sh.call("focusPane", "right")
    c.check("and so is the right pane's",
            sh.wait_state(lambda s: s.get("selection") == [dst + "/here.txt"]) is not None, sh.state().get("selection"))

    # The other way round: collapsing from the left puts the right pane away by taking its view
    # off the screen altogether, rather than by the two panes changing places.
    sh.call("focusPane", "left")
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    c.check("collapsing from the left keeps the left pane's folder and its selection",
            sh.wait_state(lambda s: s.get("split") is False and s.get("uri") == src and s.get("selection") == [src + "/one.txt"]) is not None, sh.state())
    sbs(sh, "toggle")      # two local folders: only a script may (the key wants a server open)
    st = split_state(sh)
    c.check("and opening it again brings the other folder back on its own side",
            st.get("leftUri") == src and st.get("rightUri") == dst, st)
    sh.call("focusPane", "right")
    c.check("with the selection it was put away with",
            sh.wait_state(lambda s: s.get("selection") == [dst + "/here.txt"]) is not None, sh.state().get("selection"))

    # ------------------------------------------------------- the keys scroll the pane they are in
    # Seen by the owner with a server on the right, and nothing to do with servers: the keys
    # moved the selection in the focused pane and scrolled the LEFT view whichever that was, so
    # on the right the selection walked out of sight and the list never followed. The right-hand
    # view is also built for the left pane and handed its own afterwards, and its listing was
    # never told what was on screen.
    long_l, long_r = os.path.join(ctx.root, "long-l"), os.path.join(ctx.root, "long-r")
    for folder, stem in ((long_l, "l"), (long_r, "r")):
        os.makedirs(folder)
        for i in range(60):
            open(os.path.join(folder, "%s%02d.txt" % (stem, i)), "w").write("x")
    for side, folder in (("left", long_l), ("right", long_r)):
        sh.call("focusPane", side)
        sh.open("file://" + folder)
    geo = lambda i: json.loads(sh.call("rowGeometry", str(i)) or "{}")
    for side in ("right", "left"):
        sh.call("focusPane", side)
        sh.keys(("Home",))
        c.check("%s: the last row of sixty starts out of sight" % side, wait_for(lambda: geo(0).get("onscreen") and not geo(59).get("onscreen")) is not None, geo(59))
        sh.keys(("End",))
        c.check("%s: End brings the last row into view" % side, wait_for(lambda: geo(59).get("onscreen")) is not None, geo(59))
        win = json.loads(sh.call("windowState", side) or "{}")
        c.check("%s: and its listing is told what is on screen" % side, win.get("viewport", [0, 0])[0] > 0 and win.get("viewport", [0, 60])[1] < 60, win)
        sh.keys(("Home",))
        c.check("%s: Home brings the first back" % side, wait_for(lambda: geo(0).get("onscreen")) is not None, geo(0))
    for side, folder in (("left", src), ("right", dst)):
        sh.call("focusPane", side)
        sh.open(folder)

    # -------------------------------------------------------------- a location, and its pair
    if not sshd_bin():
        print("  (no sshd — sudo pacman -S openssh: the location checks are skipped)")
        return
    servers = Servers(os.path.join(ctx.base, "servers"))
    try:
        pair = ctx.fixture({"mine": {"here.txt": "mine"}, "served": {"theirs.txt": "theirs"}})
        local, remote = "file://" + os.path.join(pair, "mine"), "sftp://e2e-sbs" + os.path.join(pair, "served")
        port, key = servers.start_sftp()
        r = add_location(d, {"name": "e2e-sbs", "plugin": "sftp", "remoteUri": remote, "localUri": local,
                             "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
        if not c.check("the SFTP location is added, its host key trusted", "ok" in r and not r["ok"].get("verify"), r):
            return

        # Clicking it in the sidebar. The window is told about a new location by an event, so the
        # click is offered until it takes rather than once, before the news has arrived.
        def click():
            sh.call("openLocation", "e2e-sbs")
            st = sbs(sh)
            return st if st.get("rightUri") == remote else None
        st = wait_for(click, timeout=20, interval=0.3, what="the location opens") or sbs(sh)
        c.check("opening a location opens it side by side", st.get("split") is True, st)
        c.check("its own local folder on the left, its server on the right",
                st.get("leftUri") == local and st.get("rightUri") == remote, st)
        c.check("and the focus on the server side, where the work is", st.get("focused") == "right", st)
        c.check("the server's folder lists in its pane",
                sh.wait_state(lambda s: s.get("uri") == remote and s.get("done") and s.get("count") == 1 and not s.get("error"), timeout=30) is not None, sh.state())
        sh.call("focusPane", "left")
        c.check("and the local one beside it",
                sh.wait_state(lambda s: s.get("uri") == local and s.get("done") and s.get("count") == 1 and not s.get("error")) is not None, sh.state())
        # Connected, the location's row in the sidebar wears a green dot (owner, 2026-09-21).
        dots = lambda: json.loads(sh.call("locationDots") or "[]")
        c.check("the connected location wears the green dot", wait_for(lambda: "e2e-sbs" in dots() or None, timeout=10) is not None, dots())

        # The sidebar menu's Disconnect: the server is let go of, the pane that was on it goes
        # to the location's local folder, and — side by side being that server and its folder —
        # it is one pane again. (It used to send the request and leave the pane on a server that
        # had gone.)
        sh.call("disconnectLocation", "e2e-sbs")
        c.check("Disconnect brings the window back to one pane, on the local folder",
                sh.wait_state(lambda s: s.get("split") is False and s.get("uri") == local) is not None, sh.state())
        c.check("and the dot goes with the connection", wait_for(lambda: "e2e-sbs" not in dots() or None, timeout=10) is not None, dots())
        c.check("and the daemon has no session left to it", "e2e-sbs" not in [l["name"] for l in d.ok("Locations")["locations"] if l.get("connected")])
        # Back on the server for what follows.
        st = wait_for(click, timeout=20, interval=0.3, what="the location opens again") or sbs(sh)
        c.check("it opens again, side by side", st.get("split") is True and st.get("rightUri") == remote, st)
        sh.wait_state(lambda s: s.get("uri") == remote and s.get("done"), timeout=30)
        sh.call("focusPane", "left")
        sh.wait_state(lambda s: s.get("uri") == local and s.get("done"))

        # Side by side is a server and the folder kept beside it (owner, 2026-09-21). Ctrl+4 with
        # the focus on the LOCAL side still leaves the server's pane: one pane is the server.
        sh.keys(("ctrl", "4"))
        c.check("one pane again, and it is the server's", sh.wait_state(lambda s: s.get("split") is False and s.get("uri") == remote) is not None, sh.state())
        sh.keys(("ctrl", "4"))
        st = split_state(sh)
        c.check("Ctrl+4 again brings both back, each on its own side",
                st.get("leftUri") == local and st.get("rightUri") == remote, st)
        # With no server open the key does nothing: there is nothing to put beside anything.
        sh.keys(("ctrl", "4"))
        sh.wait_state(lambda s: s.get("split") is False)
        sh.open(src)
        sh.keys(("ctrl", "4"))
        time.sleep(0.4)
        c.check("on a plain local folder Ctrl+4 does nothing", sh.state().get("split") is False, sh.state())
        # …so the pair the next part starts from is a script's doing.
        sbs(sh, "toggle")
        st = split_state(sh)

        # A remote URI opened on its own — here typed into the breadcrumb, as the IPC and "Show in
        # folder" also do — finds the location it belongs to and pairs it with that location's
        # local folder. Neither pane is anywhere near either of them first, so the pair has to be
        # made; and it is collapsed from the RIGHT, so the two Pane objects have changed places
        # and the one being typed into is the one the layout will have to hand back to the right.
        sh.call("focusPane", "right")
        sh.open(dst)
        sh.keys(("ctrl", "4"))
        c.check("one pane, with the server nowhere in the window",
                sh.wait_state(lambda s: s.get("split") is False and s.get("uri") == dst) is not None
                and not any(str(sbs(sh).get(k, "")).startswith("sftp://") for k in ("leftUri", "rightUri")), (sh.state().get("uri"), sbs(sh)))
        sh.call("action", "typePath")
        if c.check("the path becomes a field to type in", sh.wait_state(lambda s: s.get("keyFocus") is False) is not None, sh.state()):
            sh.type(remote)
            sh.keys(("Return",))
            st = wait_for(lambda: (lambda x: x if x.get("rightUri") == remote else None)(sbs(sh)), timeout=30, what="the pair") or sbs(sh)
            c.check("a remote URI from the breadcrumb opens side by side", st.get("split") is True, st)
            c.check("with the server on the right and its location's local folder on the left",
                    st.get("rightUri") == remote and st.get("leftUri") == local, st)
        sh.call("dismiss")
    finally:
        # Neither pane may be left standing on a server that is about to stop answering.
        for side in ("right", "left"):
            sh.call("focusPane", side)
            sh.call("open", src)
        sh.call("split", "off")
        sh.call("dismiss")
        d.call("RemoveLocation", name="e2e-sbs")
        servers.stop()
