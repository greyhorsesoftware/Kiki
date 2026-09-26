"""A demo video: what is new in this release, recorded (the version from `version.js`).

Not a test and not in the default run — `KIKI_E2E_DESKTOP=1 tests/e2e/run.sh --flow whatsnew`
records it on the compositor you are sitting at (see `demo.py` for the two ways to record and
what each needs); the file is `tests/e2e/out/kiki-whatsnew.mp4`. The 0.1.1 one is `whatsnew011`.

What it shows, in order: a title card; the Vim letters as the keys — h j k l, v, y p, dd and z,
`.`, f; Quick Look on Space — a picture, a Markdown document with its contents down the left,
a PDF page by page, a video playing, code in colour, a spreadsheet's Open with… face; a file on
a server fetched and shown; side by side with the server, both panes listing at once.
"""

import getpass, os, re, shutil, time
from harness import make_tree, wait_for
from media import png, pdf, mp4, MARKDOWN
from servers import Servers, add_location
from flows import demo
from flows.demo import DESKTOP, Desktop, Recorder, home_spec

NEEDS = {"shell", "keyboard"}
TITLE = "a demo video: what is new in this release"
probe = demo.probe

CODE = """//! Where the week's notes go.
use std::path::PathBuf;

/// One line per day, newest last.
pub fn notes_file(home: &str) -> PathBuf {
    let mut p = PathBuf::from(home);
    p.push("Documents");
    p.push("Field notes.md");
    p
}

fn main() {
    for day in ["Monday", "Tuesday"] {
        println!("{day}: {}", notes_file("/home/gideon").display());
    }
}
"""


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    out = os.environ.get("KIKI_E2E_OUT", "/tmp")
    fixture = os.environ.get("HOME_FIXTURE", ctx.base)
    home = make_tree(fixture if DESKTOP else os.path.join(fixture, "gideon"), home_spec())
    aside = os.path.join(os.path.dirname(fixture.rstrip("/")), "whatsnew")
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))   # flows → e2e → tests → the checkout
    # The release the video is for: the window's own version, never a number typed here.
    version = re.search(r'var version = "([^"]+)"', open(os.path.join(root, "qml", "kiki", "version.js"), encoding="utf-8").read()).group(1)

    # The looks: two of the owner's pictures when they are there, else made ones; a document,
    # a paper, a clip, a program, a spreadsheet — named so j walks them in this order.
    looks = os.path.join(home, "Looks"); os.makedirs(looks, exist_ok=True)
    own = os.path.expanduser("~/Pictures")
    pics = [f for f in (sorted(os.listdir(own)) if os.path.isdir(own) else []) if f.lower().endswith((".jpg", ".jpeg", ".png"))][:2]
    for i, f in enumerate(pics, 1):
        shutil.copy(os.path.join(own, f), os.path.join(looks, f"01 photo {i}{os.path.splitext(f)[1].lower()}"))
    if not pics:
        png(os.path.join(looks, "01 photo 1.png"), 1200, 800)
    open(os.path.join(looks, "02 Field notes.md"), "w").write(MARKDOWN)
    # The owner's own paper when it is there, else a made one of three pages.
    own_pdf = os.path.expanduser("~/Downloads/10-Yard-50-200-Zero-Target.pdf")
    if os.path.isfile(own_pdf):
        shutil.copy(own_pdf, os.path.join(looks, "03 zero target.pdf"))
    else:
        pdf(os.path.join(looks, "03 paper.pdf"), 3)
    # The owner's own clip when it is there (a real one reads better than a test pattern).
    own_clip = os.path.expanduser("~/Videos/spot-en.mp4")
    if os.path.isfile(own_clip):
        shutil.copy(own_clip, os.path.join(looks, "04 clip.mp4")); has_video = True
    else:
        has_video = mp4(os.path.join(looks, "04 clip.mp4"), 6)
    open(os.path.join(looks, "05 notes.rs"), "w").write(CODE)
    open(os.path.join(looks, "06 budget.xlsx"), "wb").write(b"PK\x03\x04" + b"\0" * 400)
    open(os.path.join(home, "Documents", ".drafts"), "w").write("hidden\n")

    # The server on the right, as in the demo: a short path, ours for the run.
    servers = Servers(os.path.join(aside, "servers"))
    remote_root = "/tmp/homelab"
    shutil.rmtree(remote_root, ignore_errors=True)
    os.makedirs(os.path.join(remote_root, "shared"))
    open(os.path.join(remote_root, "shared", "Field notes.md"), "w").write(MARKDOWN)
    open(os.path.join(remote_root, "shared", "todo.md"), "w").write("- fix the fence\n- count the sheep again\n")
    port, key = servers.start_sftp()
    r = add_location(d, {"name": "homelab", "plugin": "sftp", "remoteUri": "sftp://homelab" + remote_root, "localUri": "",
                         "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
    c.check("the SFTP server is up and the location added", "ok" in r and not r["ok"].get("verify"), r)
    remote_uri = "sftp://homelab" + remote_root
    docs = "file://" + os.path.join(home, "Documents")

    def beat(s):
        time.sleep(s)

    def expect(what, pred, timeout=8):
        if sh.wait_state(pred, timeout) is None:
            print(f"  ... did not see: {what}   {sh.state()}")

    def ql():
        return sh.state().get("quickLook") or {}

    desk = Desktop()
    rec = Recorder(out)
    rec.out = os.path.join(out, "kiki-whatsnew.mp4")
    rec.title = (os.path.join(root, "app-images", "kikifull.png"), f"kiki {version} — what\u2019s new", 3.5)
    sh.call("dismiss")
    sh.call("split", "off")
    sh.call("setView", "list")
    sh.open(docs)
    expect("Documents", lambda s: s.get("uri") == docs and s.get("done"))
    if DESKTOP:
        desk.enter(fullscreen=False)      # Quick Look tiles beside kiki, not over the screen
    beat(0.6)
    rec.start(desk.monitor)

    # ------------------------------------------------------------ the keys are the Vim keys
    rec.say(f"{version} — the keys are the Vim keys: h j k l move, no preference to turn on")
    sh.select("Budget 2026.csv")
    beat(1.2)
    for k in ("j", "j", "k"):
        sh.keys((k,)); beat(0.7)
    sh.select("Receipts"); beat(0.5)
    sh.keys(("l",)); beat(1.2)
    sh.keys(("h",)); beat(1.0)
    rec.say("v extends the selection; y copies, p pastes")
    sh.select("Budget 2026.csv"); beat(0.5)
    sh.keys(("v",)); beat(0.4); sh.keys(("j",)); beat(0.4); sh.keys(("j",)); beat(0.8)
    sh.keys(("y",)); beat(0.6)
    sh.select("Receipts"); sh.keys(("l",)); beat(0.8)
    sh.keys(("p",)); beat(2.0)
    sh.keys(("h",)); beat(0.8)
    rec.say("dd trashes, z brings it back")
    sh.select("Reading list.md"); beat(0.5)
    sh.keys(("d",), ("d",)); beat(1.8)
    sh.keys(("z",)); beat(1.8)
    rec.say(". shows hidden files; f filters — typing no longer jumps, it filters")
    sh.keys(("period",)); beat(1.4); sh.keys(("period",)); beat(0.8)
    sh.keys(("f",)); beat(0.4); sh.type("not"); beat(1.6); sh.keys(("Escape",)); beat(0.6)

    # ------------------------------------------------------------ quick look
    sh.open("file://" + looks)
    expect("Looks", lambda s: s.get("uri") == "file://" + looks and s.get("done"))
    sh.select("01 photo 1" + (os.path.splitext(pics[0])[1].lower() if pics else ".png"))
    beat(0.8)
    rec.say("Space: Quick Look — a window of its own, beside the list")
    sh.keys(("space",))
    expect("quick look", lambda s: (s.get("quickLook") or {}).get("visible"))
    beat(3.0)
    rec.say("j and k step through the folder without leaving it")
    sh.keys(("j",)); beat(2.2)
    sh.keys(("j",))
    expect("the document", lambda s: (s.get("quickLook") or {}).get("kind") == "markdown")
    rec.say("A Markdown document, rendered, with its contents down the left — t folds them")
    beat(2.5); sh.keys(("t",)); beat(1.2); sh.keys(("t",)); beat(1.2)
    sh.keys(("j",))
    expect("the paper", lambda s: (s.get("quickLook") or {}).get("kind") == "pdf")
    rec.say("A PDF, rendered by the daemon — + zooms in and it is rendered again, sharp; 0 fits")
    beat(2.0); sh.keys(("plus",)); beat(1.6); sh.keys(("plus",)); beat(1.6); sh.keys(("0",)); beat(1.2)
    if has_video:
        sh.keys(("j",))
        expect("the clip", lambda s: (s.get("quickLook") or {}).get("kind") == "video")
        rec.say("A video, playing — k pauses, the arrows seek")
        beat(3.5); sh.keys(("k",)); beat(1.0); sh.keys(("k",)); beat(1.5)
    sh.keys(("j",))
    expect("the code", lambda s: (s.get("quickLook") or {}).get("kind") == "text")
    rec.say("Code in colour — HTML, JavaScript, Rust, C, Java and fifty more")
    beat(3.5)
    sh.keys(("j",))
    expect("the spreadsheet", lambda s: (s.get("quickLook") or {}).get("kind") == "other")
    rec.say("What it cannot show, it says — and offers Open with…")
    beat(2.5)
    sh.keys(("Escape",))
    expect("closed", lambda s: not (s.get("quickLook") or {}).get("visible"))
    beat(0.6)

    # ------------------------------------------------------------ a server
    rec.say("Side by side with a server: both panes list at once, neither waits for the other")
    sh.call("split", "on")
    expect("side by side", lambda s: s.get("split") is True)
    sh.call("focusPane", "right")
    sh.open(remote_uri + "/shared")
    expect("the server's folder", lambda s: s.get("uri") == remote_uri + "/shared" and s.get("done"), 20)
    beat(1.5)
    sh.select("Field notes.md"); beat(0.6)
    rec.say("Space on a file on a server: fetched, then shown — nothing is left behind")
    sh.keys(("space",))
    expect("fetched and shown", lambda s: (s.get("quickLook") or {}).get("face") == "content", 20)
    beat(3.5)
    sh.keys(("Escape",)); beat(0.6)
    rec.say("A double-click on a server's file opens Quick Look too; the local one, its application")
    beat(2.5)

    # ------------------------------------------------------------ out
    rec.say(f"In English, Spanish and Japanese, by the desktop's language. kiki {version}, for Omarchy")
    sh.call("split", "off")
    expect("one pane again", lambda s: s.get("split") is False)
    sh.open("file://" + home)
    beat(2.5)

    ok = rec.finish()
    if DESKTOP:
        desk.leave()
    servers.stop()
    shutil.rmtree(remote_root, ignore_errors=True)
    c.check("the video was written", ok and os.path.getsize(rec.out) > 100_000, rec.out)
    print(f"  video: {rec.out}")
