"""A demo video: what is new in 0.1.1, recorded.

Not a test and not in the default run — `KIKI_E2E_DESKTOP=1 tests/e2e/run.sh --flow whatsnew`
records it on the compositor you are sitting at (see `demo.py` for the two ways to record and
what each needs); the file is `tests/e2e/out/kiki-whatsnew.mp4`.

What it shows, in order: Search everywhere first on the rail; the columns view's filter
narrowing the column the keyboard is in; side by side with the info panel as a card over the
other pane, on one file and then on three (the fan, the permissions grid over all three and one
Apply for the lot); a file trashed and the
message rolling into the bottom bar in the shortcuts' place, undone from there; the location's
connection log with what was said to the server and what it answered.
"""

import getpass, os, shutil, time
from harness import make_tree, wait_for
from servers import Servers, add_location
from flows import demo
from flows.demo import DESKTOP, Desktop, Recorder, home_spec, pictures_into

NEEDS = {"shell", "keyboard"}
TITLE = "a demo video: what is new in 0.1.1"
probe = demo.probe


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    out = os.environ.get("KIKI_E2E_OUT", "/tmp")
    fixture = os.environ.get("HOME_FIXTURE", ctx.base)
    home = make_tree(fixture if DESKTOP else os.path.join(fixture, "gideon"), home_spec())
    aside = os.path.join(os.path.dirname(fixture.rstrip("/")), "whatsnew")
    kikid = os.path.join(os.environ.get("KIKI_PLUGIN_DIR", ""), "kikid")
    if not os.path.exists(kikid):
        kikid = shutil.which("kikid") or "kikid"
    pictures_into(os.path.join(home, "Pictures"), kikid)

    # The server on the right, as in the demo: a short path, ours for the run.
    servers = Servers(os.path.join(aside, "servers"))
    remote_root = "/tmp/homelab"
    shutil.rmtree(remote_root, ignore_errors=True)
    os.makedirs(os.path.join(remote_root, "backups"))
    os.makedirs(os.path.join(remote_root, "shared"))
    open(os.path.join(remote_root, "shared", "todo.md"), "w").write("- fix the fence\n")
    open(os.path.join(remote_root, "backups", "2026-09-01.tar.zst"), "wb").write(b"x" * 500_000)
    port, key = servers.start_sftp()
    r = add_location(d, {"name": "homelab", "plugin": "sftp", "remoteUri": "sftp://homelab" + remote_root, "localUri": "",
                         "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
    c.check("the SFTP server is up and the location added", "ok" in r and not r["ok"].get("verify"), r)
    remote_uri = "sftp://homelab" + remote_root
    projects = "file://" + os.path.join(home, "Projects")
    docs = "file://" + os.path.join(home, "Documents")

    def beat(s):
        time.sleep(s)

    def expect(what, pred, timeout=8):
        if sh.wait_state(pred, timeout) is None:
            print(f"  ... did not see: {what}   {sh.state()}")

    def view(v):
        sh.call("setView", v)
        expect(f"view {v}", lambda s: s.get("view") == v)

    pointer = ctx.pointer if DESKTOP and shutil.which("wlrctl") else None

    def park():
        if pointer:
            pointer.warp(24, 560)      # the rail's empty lower half: nothing there to light up

    # Search everywhere reads the index, which the daemon builds a while after it starts: ask
    # for it now, over the demo home, so "report" finds the quarterly report on screen.
    d.call("SetIndexRoots", roots=["file://" + home])
    d.call("IndexRebuild")
    wait_for(lambda: ((d.call("IndexStatus") or {}).get("ok") or {}).get("entries", 0) > 10 or None, timeout=20)

    desk = Desktop()
    rec = Recorder(out)
    rec.out = os.path.join(out, "kiki-whatsnew.mp4")
    sh.call("dismiss")
    sh.call("split", "off")
    view("list")
    sh.open("file://" + home)
    if DESKTOP:
        desk.enter()
    park()
    beat(0.5)
    rec.start(desk.monitor)

    # ------------------------------------------------------- search, first on the rail
    rec.say("0.1.1 — Search everywhere is now first on the rail")
    beat(1.8)
    if not (pointer and pointer.click_name("sidebar-search", timeout=4)):
        sh.call("searchEverywhere", "")
    beat(0.3)
    park()                       # off the rail entry, so its tooltip goes before the results come
    beat(0.6)
    sh.type("report")            # into the overlay's input, which has the focus
    beat(3.0)
    sh.call("searchEverywhere", "")      # closes it: open, and nothing to search for
    beat(0.6)
    park()

    # ---------------------------------------------- the columns' filter follows the keyboard
    rec.say("In columns, the filter narrows the column the keyboard is in")
    # Columns start at the pane's folder: walk in from home a click at a time, as the demo does
    # (Projects, kiki, src), so there are columns on screen when the filter narrows the last.
    sh.open("file://" + home)
    view("columns")
    beat(1.0)
    for col, row, uri in ((0, 4, projects), (1, 1, projects + "/kiki"), (2, 1, projects + "/kiki/src")):
        if pointer and pointer.click_name(f"colrow-{col}-{row}", timeout=4) and sh.wait_geometry(f"colrow-{col + 1}-0", 4):
            beat(1.0)
            continue
        print(f"  ... the click into column {col + 1} did not open it; opening {uri} by name")
        sh.open(uri)
        view("columns")
        beat(1.0)
    park()
    # The clicks left the keyboard in kiki's column with src chosen; Right steps into src, and
    # the filter narrows THAT column — which is the point.
    sh.keys(("Right",))
    beat(0.8)
    sh.call("search", "list")
    beat(3.0)
    sh.call("search", "")
    beat(0.8)
    view("list")

    # ------------------------------------------- the info panel, side by side, as a card
    rec.say("Side by side, the info panel is a card over the other pane, pointing at the selection")
    sh.call("split", "on")
    expect("side by side", lambda s: s.get("split") is True)
    sh.call("focusPane", "right")
    sh.open(remote_uri)
    beat(1.5)
    sh.call("focusPane", "left")
    # One of the three files shown later is private (600), so the permissions grid over them
    # has something to show a dash for; set before the folder is listed, so the rows carry it.
    os.chmod(os.path.join(home, "Documents", "Notes.txt"), 0o600)
    sh.open(docs)
    beat(0.8)
    sh.select("Quarterly report.md")
    beat(0.6)
    sh.keys(("ctrl", "i"))
    expect("the card is up", lambda s: s.get("inspector") is True)
    beat(4.0)
    rec.say("Several files: a fan of what they are, and their details summed up")
    sh.call("selectMany", "Notes.txt,Budget 2026.csv,Reading list.md")
    beat(3.5)
    # Permissions over the three: the grid says where they differ (a dash), a box ticked is
    # ticked for all of them, and Apply is one job for the lot. The tab and the boxes are
    # clicked; without a pointer this part is left out.
    if pointer and pointer.click_name("tab-permissions", timeout=4):
        rec.say("Permissions for all of them at once: a dash where they differ")
        beat(3.0)
        rec.say("Tick a box, Apply — one job for the lot")
        pointer.click_name("perm-group-2", timeout=4)
        beat(1.2)
        pointer.click_name("perm-world-2", timeout=4)
        beat(1.2)
        pointer.click_name("perm-apply", timeout=4)
        beat(3.0)
        park()
    else:
        print("  ... no pointer: the permissions grid is not shown")
    sh.keys(("ctrl", "i"))
    expect("the card is down", lambda s: s.get("inspector") is False)
    beat(0.6)

    # -------------------------------------------------- the message in the bottom bar
    rec.say("Messages roll into the bottom bar, in the shortcuts' place")
    sh.select("Reading list.md")
    beat(0.6)
    sh.call("action", "trash")
    beat(3.2)
    rec.say("… and Undo is right there")
    if not (pointer and pointer.click_name("toast-undo", timeout=4)):
        sh.call("action", "undo")
    beat(2.5)
    park()

    # ------------------------------------------------------------- the connection log
    if pointer and pointer.click_name("sidebar-homelab · sftp", "right", timeout=4):
        rec.say("The connection log: what was said to the server, and what it answered")
        beat(0.8)
        if pointer.click_name("menu-Connection Log…", timeout=4):
            beat(5.0)
            sh.keys(("Escape",))
            beat(0.6)
        else:
            print("  ... the location menu had no Connection Log item")
            sh.keys(("Escape",))
    else:
        print("  ... no pointer, or no rail item to right-click: the connection log is not shown")
    park()

    # ------------------------------------------------------------------------- out
    rec.say("kiki 0.1.1, for Omarchy")
    sh.call("split", "off")
    expect("one pane again", lambda s: s.get("split") is False)
    sh.open("file://" + home)
    beat(2.0)

    ok = rec.finish()
    if DESKTOP:
        desk.leave()
    shutil.rmtree(remote_root, ignore_errors=True)
    c.check("the video was written", ok and os.path.getsize(rec.out) > 100_000, rec.out)
    print(f"  video: {rec.out}")
