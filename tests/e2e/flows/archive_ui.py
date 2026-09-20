"""Compress and extract the way a person does it: the context menu and the dialog."""
import os
from harness import snapshot, wait_for

NEEDS = {"shell", "keyboard"}
TITLE = "archives through the menu"


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    root = ctx.fixture({"work": {"one.txt": "one", "sub": {"two.txt": "two"}}, "out": {}})
    sh.open("file://" + root)
    if not c.check("the keymap has the keyboard focus", sh.state().get("keyFocus") is True, sh.state()):
        return
    original = snapshot(root + "/work")

    # Compress… opens a dialog with the name filled in; Enter accepts it.
    sh.select("work")
    sh.menu("Compress…")
    c.check("Compress opens its dialog", sh.wait_state(lambda st: st.get("dialogs", {}).get("compress")) is not None, sh.state())
    sh.keys(("Return",))
    made = wait_for(lambda: [k for k in snapshot(root) if k.startswith("work.")])
    c.check("the archive is written next to the folder", made is not None, list(snapshot(root)))
    if not made:
        return
    archive = sorted(made)[0]

    # Extract here puts the tree back beside it.
    sh.open("file://" + root + "/out")
    sh.open("file://" + root)
    sh.select(archive)
    sh.menu("Extract here")
    # `work` is still here, so what comes out lands beside it under a free name: extracting never
    # merges into a folder that was already there (plan 05).
    back = wait_for(lambda: snapshot(root + "/work (2)") if os.path.isdir(root + "/work (2)") and snapshot(root + "/work (2)") == original else None)
    c.check("extract here puts the same tree beside the original, under a free name", back == original, sorted(k for k in snapshot(root) if "/" not in k))
    c.check("and the folder that was there is untouched", snapshot(root + "/work") == original)
