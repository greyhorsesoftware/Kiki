"""Every dialog, photographed — for the localization plan's L4 (docs/0.2.0/02-localization.md):
run once per language and look at what clipped.

    KIKI_E2E_LANGUAGE=es tests/e2e/run.sh --flow photographs
    KIKI_E2E_LANGUAGE=ja tests/e2e/run.sh --flow photographs

The files land in tests/e2e/out/photo-<lang>-<name>.png. Not in the default run.
"""

import os, shutil, subprocess, time
from harness import make_tree

NEEDS = {"shell", "keyboard"}
TITLE = "photographs of every dialog, in the language asked for"


def probe(ctx):
    return None if shutil.which("grim") else "grim is not installed"


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    out = os.environ.get("KIKI_E2E_OUT", "/tmp")
    lang = os.environ.get("LANGUAGE", "en")
    home = make_tree(os.path.join(ctx.base, "home"), {
        "Documents": {"Quarterly report.md": "# Q\n" * 40, "Notes.txt": "call the dentist\n", "Budget 2026.csv": "a,b\n", "Receipts": {"march.pdf": 40_000}},
        "Pictures": {}, "Backup": {}, "Projects": {"kiki": {"README.md": "# kiki\n", "src": {"main.rs": "fn main() {}\n"}}},
    })
    shots = []

    def shot(name, settle=0.6):
        time.sleep(settle)
        path = os.path.join(out, f"photo-{lang}-{name}.png")
        subprocess.run(["grim", path], capture_output=True)
        shots.append(name)

    sh.open("file://" + home)
    time.sleep(0.8)
    shot("list")
    sh.open("file://" + os.path.join(home, "Documents")); time.sleep(0.6)
    sh.select("Quarterly report.md")
    sh.call("action", "inspector"); shot("info-panel")
    sh.call("action", "inspector")
    sh.call("selectMany", "Quarterly report.md,Notes.txt,Budget 2026.csv"); sh.call("action", "inspector"); shot("info-panel-many")
    sh.call("action", "inspector")
    sh.keys(("Escape",))
    # the context menu on a row
    # (the Menu key opens it; Escape clears the selection, so select again for each)
    sh.select("Notes.txt"); sh.keys(("Menu",)); shot("context-menu")
    sh.keys(("Escape",))
    # the confirmations
    sh.select("Notes.txt"); sh.call("action", "deleteForever"); shot("confirm-delete"); sh.call("question", "no")
    sh.select("Notes.txt"); sh.call("contextMenu", "compress"); shot("compress-dialog"); sh.keys(("Escape",))
    # side by side and the info card
    sh.call("split", "on"); time.sleep(0.6)
    # The mirror's other side is the right pane: an empty folder of ours, so the compare is five
    # items and not whatever the pane opened at (one run counted 550,500 entries of the real home).
    sh.call("focusPane", "right"); sh.open("file://" + os.path.join(home, "Backup")); time.sleep(0.6)
    sh.call("focusPane", "left"); sh.open("file://" + os.path.join(home, "Documents")); time.sleep(0.6)
    sh.select("Quarterly report.md"); sh.call("action", "inspector"); shot("info-card")
    sh.call("action", "inspector")
    sh.call("mirror", "open"); shot("mirror-configure", 1.0)
    sh.call("mirror", "preflight"); shot("mirror-review", 2.5)
    sh.call("mirror", "close"); sh.call("split", "off"); time.sleep(0.5)
    # the windows
    for page in ["general", "search", "share", "git", "project", "ai", "omarchy", "about"]:
        sh.call("settings", "open", page); shot(f"settings-{page}")
    sh.call("settings", "close"); sh.keys(("Escape",)); time.sleep(0.4)
    sh.call("action", "shortcuts"); shot("shortcuts"); sh.keys(("Escape",))
    sh.call("action", "addLocation"); shot("add-location", 1.0); sh.keys(("Escape",))
    sh.call("searchEverywhere", "report"); shot("search"); sh.call("searchEverywhere", "")
    sh.call("activity", "open"); shot("activity"); sh.call("activity", "close")
    c.check(f"{len(shots)} photographs taken in {lang}", len(shots) >= 20, shots)
    print("  photographs:", ", ".join(shots))
