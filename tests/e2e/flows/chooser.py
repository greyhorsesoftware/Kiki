"""The file chooser another application gets: where it appears, and what it answers.

Run under **sway**, not cage (`tests/e2e/run.sh`, `--part chooser`): the chooser is a layer
surface on the overlay layer since 2026-10-04 (docs/0.5.0/11-chooser-window.md), and cage has no
`wlr-layer-shell` at all — under it the dialog would be drawn nowhere and take no keys. So this
flow is local only; CI names the halves it wants and never asks for this one.

Two things are proved here that nothing else can.

**Where it appears.** The owner's fault: "did a screenshot, clicked save as and the panel showed
up behind the window I was in". A second window — `asking-app.qml`, flat magenta, full screen —
plays that application and covers the file manager's window. A chooser asked for while it is up
must be drawn over it, which is read off a screenshot rather than believed: magenta at the
corners, the chooser's own colours at the middle. Answer it and the middle is magenta again, so
the surface really goes rather than hanging about invisible with the keyboard.

**What it answers.** The portal hands the asking application a path and the application does the
writing and the reading, so this flow plays that part too (owner, 2026-10-04: "lets verify save
panel ACTUALLY saves", "same for open helper"): a Save is driven to the end and the path it
answered is written and read back; an Open is driven to the end and the file it named is read.

These were in `dbus_activation` until the chooser became a window; what is left there — the bus
starting the listener, "Show in folder" — needs no chooser and still runs under cage in CI.
"""
import json
import os
import shutil
import subprocess
import time

from harness import wait_for

NEEDS = {"shell", "keyboard"}
TITLE = "the chooser: above the application that asked, and what it answers"

LISTENER = "kiki-plugin-dbus"   # what it is built as; the package installs it as kiki-dbus
ASKER = "#ff00ff"               # asking-app.qml's colour, which no theme has


def probe(ctx):
    if not all(shutil.which(t) for t in ("dbus-run-session", "busctl")):
        return "dbus-run-session or busctl is missing"
    if not shutil.which("grim"):
        return "grim is not installed: the chooser's place cannot be read off the screen"
    return None


def listener_path(ctx):
    """The listener built for this run, beside the daemon the harness is driving."""
    for d in (os.environ.get("KIKI_PLUGIN_DIR", ""), os.path.dirname(os.environ.get("KIKI_DAEMON", "")), "target/release"):
        if d:
            p = os.path.join(d, LISTENER)
            if os.path.exists(p):
                return p
    return None


def bus_env(ctx, services):
    """An environment whose session bus can only activate what `services` holds — the developer's
    own ~/.local/share/dbus-1/services outranks XDG_DATA_DIRS, so XDG_DATA_HOME is moved too."""
    env = dict(os.environ)
    env["XDG_DATA_DIRS"] = services
    home = os.path.join(services, "home")
    os.makedirs(home, exist_ok=True)
    env["XDG_DATA_HOME"] = home
    return env


def service_file(dirpath, name, exec_path):
    services = os.path.join(dirpath, "dbus-1", "services")
    os.makedirs(services, exist_ok=True)
    with open(os.path.join(services, f"{name}.service"), "w") as f:
        f.write(f"[D-BUS Service]\nName={name}\nExec={exec_path}\n")


def wrapper(dirpath, listener):
    """What the bus starts: the listener, with its own words kept in a log a failing check prints."""
    path = os.path.join(dirpath, "start-listener")
    log = os.path.join(dirpath, "listener.log")
    with open(path, "w") as f:
        f.write(f"#!/bin/sh\nexport KIKI_DBUS_IDLE_MS=800\nexec '{listener}' >>'{log}' 2>&1\n")
    os.chmod(path, 0o755)
    return path, log


def ay(path):
    """A path as the portal spec carries one: `ay`, NUL-terminated, which busctl takes as a count
    and then the bytes. GTK sends the NUL and the listener strips it."""
    b = path.encode() + b"\0"
    return ["ay", str(len(b))] + [str(x) for x in b]


def chooser_call(env, method, handle, title, options):
    """The call another application makes, started alongside: it blocks while the person chooses,
    so it is collected after the window has answered."""
    return subprocess.Popen(
        ["dbus-run-session", "--", "busctl", "--json=short", "--user", "call",
         "org.freedesktop.impl.portal.desktop.kiki", "/org/freedesktop/portal/desktop",
         "org.freedesktop.impl.portal.FileChooser", method, "osssa{sv}",
         handle, "app.test", "", title, *options],
        env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )


def answered(proc, timeout=60):
    """`(code, uris, what it said)` from a portal call: 0 and the URIs chosen, or 1 for a
    cancellation. A call that never returns is the finding, so it is reported rather than raised."""
    try:
        out, err = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
        return None, [], "the call never returned"
    try:
        data = json.loads(out)["data"]
    except (ValueError, KeyError, TypeError):
        return None, [], (out or err).strip()[-200:]
    results = data[1] if len(data) > 1 else {}
    uris = results.get("uris", {})
    return data[0], (uris.get("data", []) if isinstance(uris, dict) else uris), (err or "").strip()[-200:]


def pixels(shot, points):
    """The colours at those points of a screenshot, as `srgb(r,g,b)` — ImageMagick reads the PNG
    so that nothing here has to decode one."""
    fmt = "".join("%[pixel:p{" + f"{x},{y}" + "}] " for x, y in points)
    r = subprocess.run(["magick", shot, "-format", fmt, "info:"], capture_output=True, text=True)
    return r.stdout.split()


def magenta(colour):
    """Allowing for the compositor's own rounding, which pixman does not do, but a scaled output
    would: the asking application's flat colour and nothing else."""
    return colour.replace(" ", "") in ("srgb(255,0,255)", "srgba(255,0,255,1)")


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    listener = listener_path(ctx)
    if not listener:
        print(f"  ... {LISTENER} was not built; skipped")
        return

    root = ctx.fixture({"Papers": {"notes.txt": "hello", "other.txt": "x"}})
    papers = os.path.join(root, "Papers")
    data = os.path.join(ctx.base, "chooser")
    os.makedirs(data, exist_ok=True)
    start, listener_log = wrapper(data, listener)
    service_file(data, "org.freedesktop.impl.portal.desktop.kiki", start)
    env = bus_env(ctx, data)
    shots = os.path.join(os.environ.get("KIKI_E2E_OUT", ctx.base), "chooser-shots")
    os.makedirs(shots, exist_ok=True)

    def said():
        try:
            return open(listener_log).read().strip()[-600:] or "(the listener said nothing)"
        except OSError:
            return "(the listener was never started)"

    def shot(name):
        """What is on the screen now. The compositor is sway and the output is its only one."""
        path = os.path.join(shots, f"{name}.png")
        subprocess.run(["grim", path], capture_output=True, timeout=30)
        return path

    def showing(what):
        """The chooser up and ready for keys. The dialog takes the focus as it opens and its list
        is still arriving; wtype types into whatever has the focus at that instant, so without a
        beat here the name lands in the window behind."""
        got = sh.wait_state(lambda s: (s.get("dialogs") or {}).get("portal") is True, 25)
        c.check(f"{what}: the chooser appears", got is not None, (sh.state().get("dialogs") or {}, said()))
        time.sleep(0.8)
        return got is not None

    def gone(what):
        c.check(f"{what}: the chooser closes", sh.wait_state(lambda s: not (s.get("dialogs") or {}).get("portal"), 10) is not None,
                (sh.state().get("dialogs") or {}))

    sh.open("file://" + papers)
    sh.wait_state(lambda s: s.get("uri") == "file://" + papers and s.get("done"))

    # ------------------------------------------------- the application that asked, over the top
    # Started here rather than in a fixture: it must be the newest window, so the compositor puts
    # it over the file manager's, which is what a chooser drawn inside that window would be under.
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    asker = subprocess.Popen(["qs", "-p", os.path.join(here, "asking-app.qml")],
                             stdout=open(os.path.join(data, "asker.log"), "w"), stderr=subprocess.STDOUT)
    covered = wait_for(lambda: (all(magenta(p) for p in pixels(shot("asker"), [(640, 360), (40, 40), (1240, 680)]))) or None, timeout=25)
    c.check("the application that asked covers the file manager's window", covered is not None,
            pixels(os.path.join(shots, "asker.png"), [(640, 360), (40, 40), (1240, 680)]))

    try:
        # ------------------------------------------------------------ a Save, above that window
        saving = chooser_call(env, "SaveFile", "/org/freedesktop/portal/desktop/request/kiki/1", "Save as",
                              ["1", "current_folder", *ay(papers)])
        showing("save")
        # 860 x 560 centred on 1280 x 720: the box runs x 210..1070, y 80..640. The middle is the
        # chooser's; the corners are still the asking application's, which proves the chooser is a
        # surface over it rather than the whole screen.
        up = pixels(shot("save-up"), [(640, 360), (640, 200), (40, 40), (1240, 680), (640, 700)])
        c.check("the chooser is drawn over the application that asked", not magenta(up[0]) and not magenta(up[1]), up)
        c.check("and only where it is: the rest of the screen is still that application", magenta(up[2]) and magenta(up[3]) and magenta(up[4]), up)

        sh.type("saved-by-kiki.txt")        # the name field has the focus in save mode
        sh.keys(("Return",))
        gone("save")
        code, uris, why = answered(saving)
        want = "file://" + os.path.join(papers, "saved-by-kiki.txt")
        c.check("a Save answers the call with one URI", code == 0 and len(uris) == 1, (code, uris, why, said()))
        c.check("and it is the folder it was given and the name that was typed", uris == [want], (uris, want))
        # The application's half: a path that cannot be written to is not a save, however the
        # dialog looked when it closed.
        saved = os.path.join(papers, "saved-by-kiki.txt")
        try:
            with open(saved, "w") as f:
                f.write("written by the application that asked")
            wrote = open(saved).read()
        except OSError as e:
            wrote = f"could not write it: {e}"
        c.check("and the application can write the file there and read it back", wrote == "written by the application that asked", wrote)
        # Nothing left behind: a layer surface that outlived its dialog would still be over the
        # screen, and worse, would still be holding the keyboard.
        after = wait_for(lambda: (all(magenta(p) for p in pixels(shot("save-gone"), [(640, 360), (640, 200)]))) or None, timeout=10)
        c.check("the surface goes with it: the screen is the asking application's again", after is not None,
                pixels(os.path.join(shots, "save-gone.png"), [(640, 360), (640, 200)]))

        # ------------------------------------------------------------ an Open that really opens
        opening = chooser_call(env, "OpenFile", "/org/freedesktop/portal/desktop/request/kiki/2", "Open a file",
                               ["1", "current_folder", *ay(papers)])
        showing("open")
        # The chooser's own keys (`PortalDialog`): j moves the selection, Return takes it.
        sh.keys(("j",))
        sh.keys(("Return",))
        gone("open")
        code, uris, why = answered(opening)
        notes = "file://" + os.path.join(papers, "notes.txt")
        c.check("an Open answers the call with the one file chosen", code == 0 and uris == [notes], (code, uris, why, said()))
        try:
            read_back = open(os.path.join(papers, "notes.txt")).read()
        except OSError as e:
            read_back = f"could not read it: {e}"
        c.check("and the application reads what the fixture wrote there", read_back == "hello", read_back)

        # ------------------------------------------------------------ more than one at a time
        many = chooser_call(env, "OpenFile", "/org/freedesktop/portal/desktop/request/kiki/3", "Open files",
                            ["2", "current_folder", *ay(papers), "multiple", "b", "true"])
        showing("multiple")
        sh.keys(("j",), ("shift", "j"))     # the first row, then the selection extended to the second
        sh.keys(("Return",))
        gone("multiple")
        code, uris, why = answered(many)
        both = [notes, "file://" + os.path.join(papers, "other.txt")]
        c.check("an Open asked for several answers with every row chosen, in order", code == 0 and uris == both, (code, uris, both, why))

        # ------------------------------------------------------------ cancelled with a real key
        # A chooser nobody answers leaves the asking application waiting for ever, so Escape must
        # reach the surface that has the keyboard and the answer must reach the bus.
        cancelling = chooser_call(env, "OpenFile", "/org/freedesktop/portal/desktop/request/kiki/4", "Open a file",
                                  ["1", "current_folder", *ay(papers)])
        showing("cancel")
        sh.keys(("Escape",))
        gone("cancel")
        code, uris, why = answered(cancelling)
        c.check("Escape cancels it and the asking application is told", code == 1 and not uris, (code, uris, why))
    finally:
        asker.terminate()
        try:
            asker.wait(timeout=10)
        except subprocess.TimeoutExpired:
            asker.kill()
