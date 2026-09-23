"""A demo video: kiki doing the basic things, recorded.

Not a test and not in the default run — `tests/e2e/run.sh --flow demo` — but it lives here
because the harness is everything a demo needs: a daemon and a shell of their own, a home built
from a spec, a real SFTP server on a port of its own, and the shell's IPC to drive it.

Two ways to record it:

  KIKI_E2E_DESKTOP=1 tests/e2e/run.sh --flow demo

runs on the compositor you are sitting at — the real renderer, theme and screen — moves the
window to an empty workspace, makes it full screen and records the monitor with
`gpu-screen-recorder` (what Omarchy's own screen recording uses). Your screen is kiki's for
about a minute. Without the variable it runs under `cage` and takes a frame every ~80 ms with
`grim` (software-rendered, 1280×720, fine for a look). Either way the file is
`tests/e2e/out/kiki-demo.mp4`.

What it shows, in order: the home folder in the list view; icon view; columns, clicked into a
project a folder at a time until a file's preview and details fill the last column; the info
panel on a document; the gallery over a folder of pictures; side by side with an SFTP server on
the right, a folder copied across and looked at there; a note edited and a file added, then the
mirror's review of exactly that, and the mirror run.
"""

import getpass, json, os, shutil, signal, subprocess, threading, time
from harness import make_tree, wait_for
from servers import Servers, add_location, sshd_bin

NEEDS = {"shell", "keyboard"}
TITLE = "a demo video: views, the info panel, the gallery, a remote"
DESKTOP = bool(os.environ.get("KIKI_E2E_DESKTOP"))
JPEGS = os.environ.get("KIKI_PERF_DIR", "/tmp/kiki-perf/gallery1k")   # gallery_perf's fixture, if it has been made
PICTURES = 40


def probe(ctx):
    if not shutil.which("ffmpeg") and not (DESKTOP and shutil.which("gpu-screen-recorder")):
        return "ffmpeg is not installed"
    if DESKTOP and not shutil.which("gpu-screen-recorder"):
        return "gpu-screen-recorder is not installed (it is what Omarchy records with)"
    if not DESKTOP and not shutil.which("grim"):
        return "grim is not installed"
    if not sshd_bin():
        return "sshd is not installed (sudo pacman -S openssh)"
    return None


# The harness gives its daemon and shell a runtime directory of their own; hyprctl finds
# Hyprland's socket, and the recorder its helpers, only in the real one.
REAL_ENV = {**os.environ, "XDG_RUNTIME_DIR": os.environ.get("KIKI_E2E_REAL_RUNTIME_DIR", os.environ.get("XDG_RUNTIME_DIR", ""))}


def hypr(*args):
    return subprocess.run(["hyprctl", *args], capture_output=True, text=True, timeout=10, env=REAL_ENV).stdout.strip()


class Desktop:
    """The window on a workspace of its own, full screen, and back again after."""

    def __init__(self):
        self.was = None
        self.monitor = None

    def enter(self):
        try:
            mons = json.loads(hypr("monitors", "-j"))
            m = next((m for m in mons if m.get("focused")), mons[0])
            self.monitor = m["name"]
            self.was = m["activeWorkspace"]["id"]
            used = {w["id"] for w in json.loads(hypr("workspaces", "-j"))}
            ws = next(i for i in range(1, 20) if i not in used)
        except (ValueError, KeyError, StopIteration, IndexError):
            return
        # Hyprland 0.56: `hyprctl dispatch` takes a Lua dispatcher call, the forms Omarchy's own
        # bindings use (/usr/share/omarchy/default/hypr/bindings/tiling.lua).
        for call in (f'hl.dsp.focus({{ window = "title:^(kiki)$" }})',
                     f'hl.dsp.window.move({{ workspace = "{ws}" }})',
                     f'hl.dsp.focus({{ workspace = "{ws}" }})',
                     f'hl.dsp.focus({{ window = "title:^(kiki)$" }})',
                     'hl.dsp.window.fullscreen({ mode = "fullscreen" })'):
            r = hypr("dispatch", call)
            if r != "ok":
                print(f"  hyprctl {call}: {r}")
            time.sleep(0.3)
        time.sleep(0.8)

    def leave(self):
        if self.was is not None:
            hypr("dispatch", f'hl.dsp.focus({{ workspace = "{self.was}" }})')


def font_file():
    r = subprocess.run(["fc-match", "-f", "%{file}", "sans-serif:bold"], capture_output=True, text=True)
    return r.stdout.strip() or None


class Recorder:
    """Frames off the screen while the demo runs, one mp4 at the end — with a line of text along
    the bottom saying what is on screen, burned in afterwards from the marks the flow leaves
    (`say`), so the shell itself draws nothing it would not draw for a user."""

    def __init__(self, out_dir):
        self.out = os.path.join(out_dir, "kiki-demo.mp4")
        self.frames = os.path.join(out_dir, "demo-frames")
        self.proc = None
        self.thread = None
        self.stop = threading.Event()
        self.n = 0
        self.t0 = 0.0
        self.marks = []          # (seconds from the start of the recording, text)

    def say(self, text):
        self.marks.append((time.monotonic() - self.t0, text))
        print(f"  {self.marks[-1][0]:5.1f}s  {text}")

    def start(self, monitor=None):
        if os.path.exists(self.out):
            os.remove(self.out)
        if DESKTOP:
            # Omarchy's own invocation (omarchy-capture-screenrecording --fullscreen), minus audio.
            self.proc = subprocess.Popen(["gpu-screen-recorder", "-w", monitor or "focused", "-s", "0x0", "-k", "auto", "-f", "60", "-fm", "cfr",
                                          "-fallback-cpu-encoding", "yes", "-o", self.out], stdout=subprocess.DEVNULL,
                                         stderr=open(os.path.join(os.path.dirname(self.out), "recorder.log"), "w"), env=REAL_ENV)
            wait_for(lambda: os.path.exists(self.out) or None, timeout=10)
            time.sleep(0.5)
            self.t0 = time.monotonic()
            return
        shutil.rmtree(self.frames, ignore_errors=True)
        os.makedirs(self.frames)
        self.t0 = time.monotonic()
        self.thread = threading.Thread(target=self._grab, daemon=True)
        self.thread.start()

    def _grab(self):
        while not self.stop.is_set():
            t = time.monotonic()
            subprocess.run(["grim", "-t", "ppm", os.path.join(self.frames, f"f{self.n:05d}.ppm")], capture_output=True)
            self.n += 1
            time.sleep(max(0.0, 0.08 - (time.monotonic() - t)))

    def finish(self):
        end = time.monotonic() - self.t0
        if self.proc:
            self.proc.send_signal(signal.SIGINT)   # SIGINT, or the file is not finalised
            try:
                self.proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.proc.kill()
            if not os.path.exists(self.out):
                return False
            return self.caption(self.out, end)
        self.stop.set()
        self.thread.join(timeout=5)
        fps = self.n / max(0.1, end) if self.n > 1 else 12
        raw = self.out + ".raw.mp4"
        r = subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-framerate", f"{fps:.3f}", "-i", os.path.join(self.frames, "f%05d.ppm"),
                            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p", "-movflags", "+faststart", raw],
                           capture_output=True, text=True)
        shutil.rmtree(self.frames, ignore_errors=True)
        if r.returncode != 0:
            print("  ffmpeg:", r.stderr.strip()[:400])
            return False
        return self.caption(raw, end)

    def caption(self, src, end):
        """Burn the marks in along the bottom: each line from its mark to the next. The text is
        drawn over a dark band a little above the status bar, sized to the frame."""
        font = font_file()
        if not self.marks or not font or not shutil.which("ffprobe"):
            if src != self.out:
                os.replace(src, self.out)
            return True
        probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=height", "-of", "csv=p=0", src],
                               capture_output=True, text=True).stdout.strip()
        h = int(probe or 1080)
        size = max(18, round(h / 36))
        filters = []
        for i, (t, text) in enumerate(self.marks):
            until = self.marks[i + 1][0] if i + 1 < len(self.marks) else end + 1
            # `expansion=none`: the text is what it says, `%` included; only the filter graph's
            # own separators want escaping, and an apostrophe is a right quote instead.
            esc = text.replace("\\", "\\\\").replace("'", "\u2019").replace(":", "\\:")
            filters.append(f"drawtext=fontfile='{font}':expansion=none:text='{esc}':fontsize={size}:fontcolor=white@0.95"
                           f":box=1:boxcolor=black@0.55:boxborderw={size // 2}:x=(w-text_w)/2:y=h-{round(h * 0.085)}-text_h"
                           f":enable='between(t,{t:.2f},{until:.2f})'")
        tmp = self.out + ".captioned.mp4"
        r = subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", src, "-vf", ",".join(filters), "-c:v", "libx264", "-preset", "medium", "-crf", "19",
                            "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-an", tmp], capture_output=True, text=True)
        if r.returncode != 0:
            print("  ffmpeg (captions):", r.stderr.strip()[:400])
            if src != self.out:
                os.replace(src, self.out)
            return True
        os.replace(tmp, self.out)
        if src != self.out and os.path.exists(src):
            os.remove(src)
        return True


def home_spec():
    src = {f"{n}.rs": f"//! {n}\n" + "fn main() {}\n" * (i + 3) for i, n in enumerate(["main", "listing", "watch", "jobs", "server"])}
    return {
        "Documents": {"Quarterly report.md": "# Quarterly report\n\nRevenue up, costs flat.\n" * 20, "Notes.txt": "call the dentist\nbuy coffee\n",
                      "Budget 2026.csv": "item,amount\nrent,1200\nfood,400\n", "Reading list.md": "- Piranesi\n- The Left Hand of Darkness\n",
                      "Receipts": {"march.pdf": 40_000, "april.pdf": 38_000}},
        "Downloads": {"omarchy-3.1.iso": 2_000_000, "kiki-0.1.0-1-x86_64.pkg.tar.zst": 7_600_000, "photo-pack.zip": 900_000, "talk-slides.pdf": 300_000},
        "Music": {"Dawn Chorus.flac": 30_000_000, "Late Trains.mp3": 8_000_000, "Weather Report - Birdland.mp3": 9_100_000},
        "Videos": {"birthday.mp4": 120_000_000, "screen recording 2026-09-20.mkv": 45_000_000},
        "Projects": {"kiki": {"Cargo.toml": "[package]\nname = \"kiki\"\n", "README.md": "# kiki\n\nA fast file manager for Omarchy.\n", "src": src,
                              "docs": {"01-daemon.md": "# 01\n", "02-shell.md": "# 02\n"}},
                     "dotfiles": {"zshrc": "export EDITOR=nvim\n", "hyprland.conf": "monitor=,preferred,auto,1\n"}},
        "Pictures": {},
    }


# The owner's own pictures first (copied into the fixture; ~/Pictures is only read) — only ones
# whose content has been looked at and belongs in a demo: landscapes, the Hubble shot, the car,
# the logo. Nothing is added here on the strength of its file name.
OWN_PICTURES = [
    ("Clifftops4-7-07.jpg", "Clifftops.jpg"),
    ("Hubble-Pillars-of-Creation.webp", "Pillars of Creation.webp"),
    ("ViewsoftheGreatSmokyMOuntainsatSunrise-WM-1200-C-1024x1024.webp", "Smoky Mountains at sunrise.webp"),
    ("giulia-quadrifoglio-honest-feedback-v0-7rz1bus7m1ge1.webp", "Giulia Quadrifoglio.webp"),
    ("greyhorse-dark.png", "greyhorse-dark.png"),
]


def pictures_into(pics, kikid):
    """The owner's pictures, then real-looking JPEGs from the gallery fixture when it exists
    (1600×1200, camera-like) up to PICTURES; otherwise the bench's 256×256 patterns."""
    own = os.path.expanduser("~/Pictures")
    n = 0
    for src, name in OWN_PICTURES:
        if os.path.isfile(os.path.join(own, src)):
            shutil.copy(os.path.join(own, src), os.path.join(pics, name))
            n += 1
    if os.path.isdir(JPEGS) and len(os.listdir(JPEGS)) >= PICTURES:
        for i, f in enumerate(sorted(os.listdir(JPEGS))[:PICTURES - n]):
            shutil.copy(os.path.join(JPEGS, f), os.path.join(pics, f"Trip to Lisbon {i + 1:02d}.jpg"))
        return
    if n:
        return
    subprocess.run([kikid, "bench", "gen", "photos", pics], capture_output=True, timeout=120)
    for f in sorted(os.listdir(pics))[PICTURES:]:
        os.remove(os.path.join(pics, f))
    for i, f in enumerate(sorted(os.listdir(pics))):
        os.rename(os.path.join(pics, f), os.path.join(pics, f"Trip to Lisbon {i + 1:02d}.png"))


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    out = os.environ.get("KIKI_E2E_OUT", "/tmp")
    # On the desktop the shell's HOME is the fixture itself (run.sh), so the breadcrumb starts at
    # the home icon; under cage it is a folder with a person's name, since the path shows anyway.
    fixture = os.environ.get("HOME_FIXTURE", ctx.base)
    home = make_tree(fixture if DESKTOP else os.path.join(fixture, "gideon"), home_spec())
    # The server's tree and keys live beside the home, not in it: nothing of the harness's is a
    # folder the demo scrolls past.
    aside = os.path.join(os.path.dirname(fixture.rstrip("/")), "demo")
    kikid = os.path.join(os.environ.get("KIKI_PLUGIN_DIR", ""), "kikid")
    if not os.path.exists(kikid):
        kikid = shutil.which("kikid") or "kikid"
    pics = os.path.join(home, "Pictures")
    pictures_into(pics, kikid)

    # The server the right pane will show: a home of its own with a little in it.
    servers = Servers(os.path.join(aside, "servers"))
    # A short path on the server, since every crumb of it is on screen: /tmp/homelab, ours for
    # the run and removed after.
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

    def beat(s):
        time.sleep(s)

    def expect(what, pred, timeout=8):
        """A wait that says so when it fails, so a demo that went wrong says where."""
        if sh.wait_state(pred, timeout) is None:
            print(f"  ... did not see: {what}   {sh.state()}")

    def view(v):
        # By IPC, not the key: the gallery keeps the keyboard, and a chord sent from inside it is
        # its own — Ctrl+2 after the gallery used to leave the view where it was.
        sh.call("setView", v)
        expect(f"view {v}", lambda s: s.get("view") == v)

    # The pointer is real on the desktop (wlrctl, corrected against `hyprctl cursorpos`) and
    # recorded, so the columns are walked by clicking; under cage there is none, and the
    # folders are opened by name instead.
    pointer = ctx.pointer if DESKTOP and shutil.which("wlrctl") else None

    def click_row(col, row, fallback_uri):
        """Click a folder's row in column `col` and wait for the column it opens (the pane's uri
        does not move when a column opens; the next column's first row is the signal). With no
        pointer, or when the click did not land, open the folder by name and come back to the
        columns, since opening a folder outright lands it in its own remembered view."""
        if pointer and pointer.click_name(f"colrow-{col}-{row}", timeout=4):
            if sh.wait_geometry(f"colrow-{col + 1}-0", 4):
                return
            print(f"  ... the click on colrow-{col}-{row} opened no column; opening it by name")
        sh.open(fallback_uri)
        view("columns")

    def info_panel(seconds):
        sh.keys(("ctrl", "i"))
        expect("info panel open", lambda s: s.get("inspector") is True)
        beat(seconds)
        sh.keys(("ctrl", "i"))
        expect("info panel closed", lambda s: s.get("inspector") is False)

    desk = Desktop()
    rec = Recorder(out)
    sh.call("dismiss")
    sh.open("file://" + home)
    if DESKTOP:
        desk.enter()
    if pointer:
        pointer.warp(1100, 640)      # out of the way, bottom right, until it is wanted
    beat(0.5)
    rec.start(desk.monitor)

    def park():
        if pointer:
            pointer.warp(1190, 668)    # on the status bar, out of every view's way

    # -------------------------------------------------------------- the home, three ways
    rec.say("Your home folder, in the list view")
    beat(2.5)
    rec.say("The same folder as icons")
    view("icon")
    beat(2.5)
    rec.say("Columns: click into a project, one folder at a time")
    view("columns")
    beat(1.2)
    # Walk into the project a click at a time — Projects, kiki, src — each opening a column.
    projects = "file://" + os.path.join(home, "Projects")
    click_row(0, 4, projects)
    beat(1.2)
    click_row(1, 1, projects + "/kiki")
    beat(1.2)
    click_row(2, 1, projects + "/kiki/src")
    beat(1.0)
    if pointer and pointer.click_name("colrow-3-1", timeout=4):
        sh.wait_state(lambda s: any(u.endswith("/listing.rs") for u in s.get("selection", [])), 3)
    else:
        sh.select("listing.rs")
    # A file in the columns puts its preview and its details in the last column — the info
    # panel itself is a list-and-icons thing (Ctrl+I is off in the columns, by design).
    rec.say("A file fills the last column with its preview and details")
    beat(3.5)
    park()

    # ------------------------------------------------------ the info panel on a document
    rec.say("The info panel, Ctrl+I, on a document")
    view("list")
    sh.open("file://" + os.path.join(home, "Documents"))
    beat(0.8)
    sh.select("Quarterly report.md")
    beat(0.6)
    info_panel(3.0)
    beat(0.5)

    # ---------------------------------------------------------------------- the gallery
    rec.say("A folder of pictures, in the gallery")
    park()
    sh.open("file://" + pics)
    beat(1.2)
    view("gallery")
    beat(2.5)
    print("  gallery:", sh.call("galleryStats"))
    for _ in range(6):
        sh.call("gallery", "next")
        beat(0.9)
    beat(1.0)
    view("list")
    sh.open("file://" + home)
    beat(1.0)

    # ------------------------------------------------------------ a remote, side by side
    rec.say("Side by side, with an SFTP server on the right")
    sh.call("sideBySide", "toggle")
    expect("side by side", lambda s: s.get("split") is True)
    beat(0.8)
    sh.call("focusPane", "right")
    sh.open(remote_uri)
    beat(2.5)
    sh.call("focusPane", "left")
    sh.select("Documents")
    beat(1.0)
    rec.say("Copy a folder across to the server")
    sh.call("transfer", "copy")

    def copied():
        try:
            jobs = json.loads(sh.call("activity") or "[]")
        except json.JSONDecodeError:
            return None
        return (jobs and all(j.get("state") in ("done", "failed", "cancelled") for j in jobs)) or None
    wait_for(copied, timeout=60)
    c.check("Documents was copied to the server", os.path.isdir(os.path.join(remote_root, "Documents")), os.listdir(remote_root))
    beat(2.0)
    # Look at what landed: the folder on the server, then inside it. (This also outlasts the
    # copy's toast, eight seconds, which would otherwise sit over the mirror's footer.)
    sh.call("focusPane", "right")
    sh.open(remote_uri + "/Documents")
    beat(2.0)
    sh.open(remote_uri + "/Documents/Receipts")
    beat(2.0)
    sh.open(remote_uri)
    beat(1.5)

    # ------------------------------------------------- work on a file, then mirror it across
    # Something has changed since the copy: a note edited, a new file beside it. The mirror's
    # review says so — one changed, one new, the rest unchanged — and then it is run.
    rec.say("Edit a note and add a file, then mirror the folder")
    docs = os.path.join(home, "Documents")
    with open(os.path.join(docs, "Notes.txt"), "a") as f:
        f.write("book the ferry\n")
    with open(os.path.join(docs, "Invoice 0042.md"), "w") as f:
        f.write("# Invoice 0042\n\nDesign work, September: 12 h\n")
    sh.call("focusPane", "left")
    sh.open("file://" + docs)
    sh.call("focusPane", "right")
    sh.open(remote_uri + "/Documents")
    beat(1.5)
    sh.keys(("ctrl", "m"))
    wait_for(lambda: (json.loads(sh.call("mirror", "state") or "{}").get("open") or None), timeout=10)
    beat(2.0)
    sh.call("mirror", "preflight")
    wait_for(lambda: (json.loads(sh.call("mirror", "state") or "{}").get("screen") == "review") or None, timeout=30)
    rec.say("The review: one changed, one new, the rest untouched")
    beat(4.0)
    rec.say("Mirror it")
    sh.call("mirror", "run")
    wait_for(lambda: ((json.loads(sh.call("mirror", "state") or "{}").get("run") or {}).get("state") in ("done", "failed") or None), timeout=60)
    c.check("the mirror brought the new file across", os.path.isfile(os.path.join(remote_root, "Documents", "Invoice 0042.md")), os.listdir(os.path.join(remote_root, "Documents")))
    beat(3.5)
    sh.call("mirror", "close")
    beat(0.8)
    rec.say("kiki, for Omarchy")
    sh.call("sideBySide", "toggle")
    expect("one pane again", lambda s: s.get("split") is False)
    sh.open("file://" + home)
    beat(2.0)

    ok = rec.finish()
    if DESKTOP:
        desk.leave()
    shutil.rmtree(remote_root, ignore_errors=True)
    c.check("the video was written", ok and os.path.getsize(rec.out) > 100_000, rec.out)
    print(f"  video: {rec.out}")
