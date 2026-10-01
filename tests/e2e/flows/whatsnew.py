"""The 0.4 what's-new video (owner, 2026-09-28: "show quicklook, then show the improved open /
save as panels, then show a list of core internal improvements").

Not a test and not in the default run — `KIKI_E2E_DESKTOP=1 tests/e2e/run.sh --flow whatsnew`
records it on the compositor you are sitting at (see `demo.py` for the two ways to record and
what each needs); the file is `tests/e2e/out/kiki-whatsnew.mp4`, the raw capture beside it. The
0.2 one is `whatsnew020`.

What it shows, in order: a title card with the About mark; a folder of pictures, paper and a
clip, and Quick Look over it — a picture, a document, the paper, the clip — in a window of its
own beside the list; another application's Open dialog, which is kiki's, walked with the keys
and answered, then its Save dialog; a card in the middle of the picture listing what 0.3 and 0.4
changed underneath, which no frame shows; an end card. Quick Look tiles beside the window, so
the window is left tiled rather than made full screen, and the crop is the tiled window's place
— which is the whole workspace, and where both windows sit when there are two.
"""

import json, os, shutil, time
from harness import make_tree
from media import pdf, mp4, MARKDOWN
from flows import demo
from flows.demo import DESKTOP, Desktop, Recorder, home_spec, pictures_into

NEEDS = {"shell", "keyboard"}
TITLE = "the 0.4 what's-new video"


def probe(ctx):
    return demo.probe(ctx)


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    out = os.environ.get("KIKI_E2E_OUT", "/tmp")
    fixture = os.environ.get("HOME_FIXTURE", ctx.base)
    home = make_tree(fixture if DESKTOP else os.path.join(fixture, "gideon"), home_spec())
    # The release the video is for. Fixed, not read from the tree: the tree moved on to 0.5.0 the
    # day after this was recorded, and a re-recording of the 0.4 video must still say 0.4.
    label = "0.4"
    kikid = os.path.join(os.environ.get("KIKI_PLUGIN_DIR", ""), "kikid")
    if not os.path.exists(kikid):
        kikid = shutil.which("kikid") or "kikid"

    # The looks: the owner's pictures (the ones demo.py has looked at), a note, paper and the clip.
    looks = os.path.join(home, "Looks"); os.makedirs(looks, exist_ok=True)
    pictures_into(looks, kikid)
    open(os.path.join(looks, "Field notes.md"), "w").write(MARKDOWN)
    own_pdf = os.path.expanduser("~/Downloads/10-Yard-50-200-Zero-Target.pdf")
    paper = "Zero target.pdf" if os.path.isfile(own_pdf) else "Paper.pdf"
    if os.path.isfile(own_pdf):
        shutil.copy(own_pdf, os.path.join(looks, paper))
    else:
        pdf(os.path.join(looks, paper), 3)
    own_clip = os.path.expanduser("~/Videos/spot-en.mp4")
    if os.path.isfile(own_clip):
        shutil.copy(own_clip, os.path.join(looks, "Spot.mp4"))
    else:
        mp4(os.path.join(looks, "Spot.mp4"), 6)
    first_picture = next((n for n in sorted(os.listdir(looks)) if n.lower().endswith((".jpg", ".png", ".webp"))), None)

    def beat(s):
        time.sleep(s)

    def expect(what, pred, timeout=8):
        if sh.wait_state(pred, timeout) is None:
            print(f"  ... did not see: {what}   {sh.state()}")

    def ql(s):
        return s.get("quickLook") or {}

    def chooser_up(s):
        return (s.get("dialogs") or {}).get("portal") is True

    desk = Desktop()
    rec = Recorder(out)
    rec.out = os.path.join(out, "kiki-whatsnew.mp4")
    rec.title = (None, f"kiki {label}", 4.0, "Quick Look, the file chooser — and what changed underneath")
    rec.end = (f"kiki {label}", 4.5, "github.com/greyhorsesoftware/Kiki")
    ok = False
    try:
        sh.call("dismiss")
        sh.call("split", "off")
        sh.call("inspector", "off")
        sh.call("setView", "list")
        sh.open("file://" + looks)
        expect("Looks", lambda s: s.get("uri") == "file://" + looks and s.get("done"))
        if first_picture:
            sh.select(first_picture)
        if DESKTOP:
            desk.enter(fullscreen=False)      # Quick Look tiles beside kiki, not over the screen
            desk.park_pointer()
            rec.crop = desk.crop              # the workspace: the window alone, or both windows
        beat(0.8)
        rec.start(desk.monitor)
        beat(2.0)

        # ------------------------------------------------------------------------ quick look
        rec.say("Quick Look", "Space on a file: a window of its own beside the list. It follows the selection.")
        beat(1.2)
        sh.keys(("space",))
        expect("quick look", lambda s: ql(s).get("visible"))
        beat(3.5)
        sh.select("Field notes.md")
        expect("the document", lambda s: ql(s).get("kind") == "markdown")
        rec.say("Quick Look", "A document, rendered, with its headings down the side.")
        beat(3.5)
        sh.select(paper)
        expect("the paper", lambda s: ql(s).get("kind") == "pdf")
        rec.say("Quick Look", "A PDF, drawn by the daemon — the same decoder that makes its thumbnail.")
        beat(3.5)
        sh.select("Spot.mp4")
        expect("the clip", lambda s: ql(s).get("kind") == "video")
        rec.say("Quick Look", "A video, playing. Escape, and it is gone.")
        beat(4.0)
        sh.keys(("Escape",))
        expect("closed", lambda s: not ql(s).get("visible"))
        beat(1.2)

        # ------------------------------------------------------------------ the file chooser
        # What another application gets when it asks the desktop for a file: the request goes to
        # the window the way the listener sends it, and the dialog is answered with the keys.
        rec.say("One file chooser", "Another app's Open dialog is kiki's: the same list, . for hidden files, / to filter.")
        sh.call("dbus", "ShowChooser", json.dumps({"token": "whatsnew-open", "mode": "open", "title": "Open a file", "currentFolder": looks}))
        expect("the open chooser", chooser_up)
        beat(2.5)
        for _ in range(3):
            sh.keys(("j",)); beat(0.7)
        beat(1.2)
        sh.keys(("slash",)); beat(0.5)
        sh.type("notes"); beat(2.2)
        # Return in the filter is the way to Search everywhere, as in the window; Escape clears
        # the text and a second closes the bar, then the keys are the list's again.
        sh.keys(("Escape",)); beat(0.4); sh.keys(("Escape",)); beat(0.8)
        sh.keys(("j",)); beat(0.9)
        sh.keys(("Return",))
        expect("the file chosen", lambda s: not chooser_up(s))
        beat(1.5)
        rec.say("One file chooser", "And its Save dialog: the name above, the folder it goes in below.")
        sh.call("dbus", "ShowChooser", json.dumps({"token": "whatsnew-save", "mode": "save", "title": "Save as", "currentFolder": looks, "currentName": "Field notes (revised).md"}))
        expect("the save chooser", chooser_up)
        beat(4.5)
        sh.keys(("Return",))
        expect("saved", lambda s: not chooser_up(s))
        beat(1.2)

        # ---------------------------------------------------------------------- under the hood
        # A quiet frame — the home, nothing selected — and the card over it for as long as it is up.
        sh.open("file://" + home)
        expect("home", lambda s: s.get("uri") == "file://" + home and s.get("done"))
        beat(1.0)
        rec.card(f"0.3 and {label} were under the hood", [
            "kiki starts its own daemon and it leaves after the last window — nothing to enable",
            "one file chooser: another app's Open and Save dialog is kiki's",
            "drag and drop shows what it does: the files under the pointer, a + for a copy",
            "folder tunnelling: hold over a folder and it opens; out, and you are back",
            "files in flight are dimmed while they are written",
            "one decoder for thumbnails and Quick Look; a bad file is tried again in a day",
            "the daemon keeps its own log",
            "an Omarchy store package",
        ], 13.0)
        beat(14.0)

        ok = rec.finish()
    finally:
        if DESKTOP:
            desk.leave()
    c.check("the video was written", ok and os.path.getsize(rec.out) > 100_000, rec.out)
    print(f"  video: {rec.out}")
