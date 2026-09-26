"""Quick Look (docs/0.2.0/05-quicklook.md): Space on a file opens a window of its own showing
the file — an image, a PDF, a video, a Markdown document, plain text — and Space again or Esc
closes it; the window follows the selection; a file on a server is fetched first; a kind it
cannot show gets the face with Open with… and starts nothing.
"""
import getpass, os, shutil, time
from harness import wait_for
from media import png, pdf, mp4, MARKDOWN
from servers import Servers, add_location

NEEDS = {"shell", "keyboard"}
TITLE = "quick look: space shows the file"


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    root = ctx.fixture({"notes.txt": "call the dentist\n" * 3, "Field notes.md": MARKDOWN, "sheet.xlsx": "PK\x03\x04not really", "Projects": {}})
    png(os.path.join(root, "photo.png")); pdf(os.path.join(root, "paper.pdf"))
    has_video = mp4(os.path.join(root, "clip.mp4"))
    uri = lambda *parts: "file://" + os.path.join(root, *parts)
    ql = lambda: (sh.state().get("quickLook") or {})
    sh.open(uri()); sh.call("setView", "list")
    if not c.check("photo.png is chosen", sh.select("photo.png") is not None, sh.state()):
        return

    # ---------------------------------------------------------------- open, close, follow
    sh.keys(("space",))
    c.check("Space opens Quick Look on the picture", sh.wait_state(lambda s: (s.get("quickLook") or {}).get("visible") and (s.get("quickLook") or {}).get("kind") == "image", 5) is not None, ql())
    sh.keys(("space",))
    c.check("Space again closes it", sh.wait_state(lambda s: not (s.get("quickLook") or {}).get("visible"), 3) is not None, ql())
    sh.keys(("space",))
    sh.wait_state(lambda s: (s.get("quickLook") or {}).get("visible"), 5)
    sh.keys(("j",))
    c.check("the window follows the selection", sh.wait_state(lambda s: (s.get("quickLook") or {}).get("uri") != uri("photo.png") and (s.get("quickLook") or {}).get("visible"), 5) is not None, ql())
    sh.keys(("Escape",))
    c.check("Esc closes it", sh.wait_state(lambda s: not (s.get("quickLook") or {}).get("visible"), 3) is not None, ql())

    # ---------------------------------------------------------------- every kind
    kinds = [("Field notes.md", "markdown"), ("paper.pdf", "pdf"), ("notes.txt", "text"), ("sheet.xlsx", "other")]
    if has_video:
        kinds.append(("clip.mp4", "video"))
    for name, kind in kinds:
        sh.select(name); sh.call("quickLook", "open")
        c.check(f"{name} shows as {kind}", sh.wait_state(lambda s, k=kind: (s.get("quickLook") or {}).get("visible") and (s.get("quickLook") or {}).get("kind") == k, 6) is not None, ql())
        sh.call("quickLook", "close")
        sh.wait_state(lambda s: not (s.get("quickLook") or {}).get("visible"), 3)
    sh.select("Projects"); sh.keys(("space",)); time.sleep(0.6)
    c.check("Space on a folder opens nothing", not ql().get("visible"), ql())

    # ---------------------------------------------------------------- nothing started
    log = os.environ.get("KIKI_E2E_OPEN_LOG")
    if log:
        c.check("no application was started by any of it", not os.path.exists(log) or os.path.getsize(log) == 0, log)

    # ---------------------------------------------------------------- a file on a server
    servers = Servers(os.path.join(ctx.base, "ql-servers"))
    remote_root = os.path.join(ctx.base, "ql-remote")
    os.makedirs(remote_root, exist_ok=True)
    open(os.path.join(remote_root, "remote.md"), "w").write(MARKDOWN)
    port, key = servers.start_sftp()
    r = add_location(d, {"name": "qlhost", "plugin": "sftp", "remoteUri": "sftp://qlhost" + remote_root, "localUri": "",
                         "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
    if c.check("an SFTP location is up", "ok" in r and not r["ok"].get("verify"), r):
        sh.open("sftp://qlhost" + remote_root)
        if sh.wait_state(lambda s: s.get("done") and s.get("count", 0) >= 1, 15) is not None and sh.select("remote.md") is not None:
            sh.keys(("space",))
            c.check("a file on a server is fetched and shown", sh.wait_state(lambda s: (s.get("quickLook") or {}).get("visible") and (s.get("quickLook") or {}).get("kind") == "markdown" and (s.get("quickLook") or {}).get("face") == "content", 20) is not None, ql())
            time.sleep(0.5)
            c.check("the fetch is nobody's toast", not sh.state().get("toast"), sh.state().get("toast"))
            sh.keys(("Escape",))
            sh.wait_state(lambda s: not (s.get("quickLook") or {}).get("visible"), 3)
            # A double-click (Enter) on a server's file is Quick Look too, never an application.
            sh.select("remote.md"); sh.keys(("Return",))
            c.check("Enter on a file on a server opens Quick Look", sh.wait_state(lambda s: (s.get("quickLook") or {}).get("visible") and (s.get("quickLook") or {}).get("kind") == "markdown", 10) is not None, ql())
            sh.keys(("Escape",))
            sh.wait_state(lambda s: not (s.get("quickLook") or {}).get("visible"), 3)
            if log:
                c.check("and no application was started for it", not os.path.exists(log) or os.path.getsize(log) == 0, log)
        else:
            c.check("the server's folder listed", False, sh.state())
    servers.stop()
    sh.open(uri())
