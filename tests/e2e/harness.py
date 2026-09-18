"""Shared machinery for the end-to-end flows (plan 28).

Three things a flow needs and nothing more: a daemon it can ask questions, a shell it can drive,
and a temp tree it can compare before and after. Everything waits on a condition — there are no
sleeps in here, because a sleep is a flake waiting for a slower machine.
"""
import json, os, socket, subprocess, sys, time

TIMEOUT = float(os.environ.get("KIKI_E2E_TIMEOUT", "8"))


def wait_for(pred, timeout=TIMEOUT, interval=0.02, what=""):
    """Poll until pred() is truthy; returns its value, or None when it never came true."""
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        v = pred()
        if v:
            return v
        time.sleep(interval)
    return None


# ---------------------------------------------------------------------------- the daemon

class Daemon:
    """kikid over its socket: newline-delimited JSON, one reply per request, events in between."""

    def __init__(self, path, client="e2e"):
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.s.connect(path)
        self.buf = b""
        self.n = 1
        self.events = []
        self.call("Hello", version=1, client=client)
        # Job events go only to clients that ask for them, and half the assertions here are
        # "the job finished".
        self.call("JobEvents")

    # A timed-out socket poisons a file object made with makefile(), and this one has to survive
    # every drain, so the framing is done here: read chunks, split on the newline.
    def _line(self, timeout=None):
        while b"\n" not in self.buf:
            self.s.settimeout(timeout)
            try:
                chunk = self.s.recv(65536)
            except (socket.timeout, TimeoutError):
                return None
            finally:
                self.s.settimeout(None)
            if not chunk:
                raise RuntimeError("daemon closed the connection")
            self.buf += chunk
        line, _, self.buf = self.buf.partition(b"\n")
        return line

    def call(self, type, **fields):
        i = self.n
        self.n += 1
        self.s.sendall((json.dumps(dict(id=i, type=type, **fields)) + "\n").encode())
        while True:
            line = self._line(TIMEOUT)
            if line is None:
                raise RuntimeError(f"{type} never answered")
            m = json.loads(line)
            if m.get("id") == i:
                return m
            self.events.append(m)

    def ok(self, type, **fields):
        """Like call, but insists on success and hands back the result object."""
        r = self.call(type, **fields)
        if "ok" not in r:
            raise RuntimeError(f"{type} failed: {r.get('err')}")
        return r["ok"]

    def drain(self, timeout=0.05):
        while True:
            line = self._line(timeout)
            if line is None:
                return
            self.events.append(json.loads(line))

    def wait_event(self, pred, timeout=TIMEOUT):
        def look():
            self.drain(0.05)
            return next((e for e in self.events if pred(e)), None)
        return wait_for(look, timeout)

    def wait_job(self, job, state="done", timeout=TIMEOUT):
        return self.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] == state, timeout)

    def submit(self, op, wait=True):
        """Submit an operation and, by default, wait for the job to finish."""
        job = self.ok("Submit", op=op)["job"]
        if wait:
            self.wait_job(job)
        return job


# ---------------------------------------------------------------------------- the shell

class Shell:
    """The Quickshell front end over `qs ipc`."""

    def __init__(self, config_dir=None, pid=None):
        self.config = config_dir or os.environ.get("KIKI_SHELL_DIR", "qml")
        self.pid = pid

    def call(self, *args):
        cmd = ["qs"]
        if self.pid:
            cmd += ["--pid", str(self.pid)]
        else:
            cmd += ["-p", self.config + "/shell.qml"]
        cmd += ["ipc", "call", "shell", *[str(a) for a in args]]
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
        return r.stdout.strip()

    def state(self):
        try:
            return json.loads(self.call("state") or "{}")
        except json.JSONDecodeError:
            return {}

    def wait_state(self, pred, timeout=TIMEOUT):
        return wait_for(lambda: pred(self.state()) or None, timeout)

    def open(self, uri):
        self.call("open", uri)
        return self.wait_state(lambda s: s.get("uri") == uri and s.get("done"))

    def select(self, name, timeout=TIMEOUT):
        """Select a row by name, retrying while we wait: a file that has just been written may
        not be in the listing the moment we ask for it, and asking once selects nothing."""
        def try_once():
            self.call("select", name)
            return any(u.endswith("/" + name) for u in self.state().get("selection", [])) or None
        return wait_for(try_once, timeout, interval=0.15)

    def geometry(self, name):
        """The rectangle of a named element, or of a row: "row-2" asks the view which delegate
        is showing row 2, which a name search cannot answer while delegates are pooled."""
        if name.startswith("row-") and name[4:].isdigit():
            call = ("rowGeometry", name[4:])
        else:
            call = ("geometry", name)
        try:
            g = json.loads(self.call(*call) or "{}")
        except json.JSONDecodeError:
            return None
        # An element scrolled out of its list still reports a rectangle; aiming at it would hit
        # whatever is really at those coordinates.
        return g if g.get("onscreen", g.get("visible")) else None

    def wait_geometry(self, name, timeout=TIMEOUT):
        return wait_for(lambda: self.geometry(name), timeout)

    def menu(self, label):
        """Invoke a context-menu item by its label, the way a click on it would."""
        self.call("contextMenu", label)

    # Each wtype run creates a virtual keyboard and uploads a keymap; sending the chord in the
    # same breath races the compositor, and the keypress is silently dropped. -s gives it a beat.
    SETTLE_MS = "40"

    def keys(self, *chords):
        """Type key chords with wtype: ('ctrl', 'c'), ('shift', 'Delete'), ('F2',) …"""
        for chord in chords:
            mods = list(chord[:-1])
            key = chord[-1]
            cmd = ["wtype", "-s", self.SETTLE_MS]
            for m in mods:
                cmd += ["-M", m]
            cmd += ["-k", key]
            for m in reversed(mods):
                cmd += ["-m", m]
            subprocess.run(cmd, check=False, timeout=10)

    def type(self, text):
        subprocess.run(["wtype", "-s", self.SETTLE_MS, "-d", "5", text], check=False, timeout=10)


class Pointer:
    """The pointer, through the wlroots virtual-pointer protocol.

    `wlrctl` only moves relatively and only clicks (press and release together), so absolute
    aiming is "park at the corner, then step to the target", and a drag — which needs the button
    held across the motion — is not possible with it; that wants ydotool.
    """

    def __init__(self, shell, origin=None):
        self.shell = shell
        if origin is None:
            env = os.environ.get("KIKI_E2E_POINTER_ORIGIN", "0,0").split(",")
            origin = (int(env[0]), int(env[1]))
        self.origin = origin

    def _run(self, *args):
        subprocess.run(["wlrctl", "pointer", *args], check=False, timeout=10)

    def warp(self, x, y):
        tx, ty = int(x) + self.origin[0], int(y) + self.origin[1]
        # Parking at the corner assumes the compositor clamps to the origin of the desktop; with
        # more than one output it clamps to the current one instead and the aim drifts by
        # whatever the offset is. Where the compositor can say where the cursor is, ask.
        where = os.environ.get("KIKI_E2E_CURSORPOS_CMD")
        if where:
            # Relative motion plus a compositor that reports the cursor lazily is a moving
            # target, so aim, read back, and correct until it is actually there.
            for _ in range(5):
                at = self._cursor(where)
                if at is None:
                    break
                if at == (tx, ty):
                    return
                self._run("move", str(tx - at[0]), str(ty - at[1]))
                time.sleep(0.05)
            else:
                return
        self._run("move", "-10000", "-10000")
        self._run("move", str(tx), str(ty))

    # The move and the click are separate processes: without a beat between them the button
    # press reaches the compositor before the motion does, and every click lands on the previous
    # target rather than this one.
    SETTLE = 0.1

    def _cursor(self, where):
        out = subprocess.run(where.split(), capture_output=True, text=True, timeout=10).stdout
        try:
            x, y = (int(v) for v in out.replace(" ", "").split(","))
            return (x, y)
        except ValueError:
            return None

    def click(self, button="left"):
        time.sleep(self.SETTLE)
        self._run("click", button)

    def click_at(self, x, y, button="left"):
        self.warp(x, y)
        self.click(button)

    def click_name(self, name, button="left", timeout=TIMEOUT):
        """Click the centre of the element with this objectName. False when it never appeared."""
        g = self.stable_geometry(name, timeout)
        if not g:
            return False
        self.click_at(g["cx"], g["cy"], button)
        return True

    def stable_geometry(self, name, timeout=TIMEOUT):
        """The element's rectangle, once it has stopped moving: a view that has just been given
        a new folder is still laying out, and a click aimed at where a row was lands on its
        neighbour."""
        g = self.shell.wait_geometry(name, timeout)
        for _ in range(5):
            if not g:
                return None
            time.sleep(0.08)
            again = self.shell.geometry(name)
            if again == g:
                return g
            g = again
        return g

    def double_click_name(self, name, timeout=TIMEOUT):
        g = self.stable_geometry(name, timeout)
        if not g:
            return False
        self.warp(g["cx"], g["cy"])
        self.click()
        self._run("click", "left")      # the second click has to follow at once to count as one
        return True


# ---------------------------------------------------------------------------- trees

def make_tree(root, spec):
    """Build a fixture. A dict is a folder, a string is a file's contents, an int is that many
    bytes of filler."""
    os.makedirs(root, exist_ok=True)
    for name, v in spec.items():
        p = os.path.join(root, name)
        if isinstance(v, dict):
            make_tree(p, v)
        elif isinstance(v, int):
            with open(p, "wb") as fh:
                fh.write(b"x" * v)
        else:
            with open(p, "w") as fh:
                fh.write(v)
    return root


def snapshot(root):
    """path -> "d" for a folder, or the file's bytes. What `diff -r` would compare, as data."""
    out = {}
    for dirpath, dirnames, filenames in os.walk(root):
        for d in dirnames:
            rel = os.path.relpath(os.path.join(dirpath, d), root)
            out[rel] = "d"
        for f in filenames:
            full = os.path.join(dirpath, f)
            rel = os.path.relpath(full, root)
            try:
                with open(full, "rb") as fh:
                    out[rel] = fh.read()
            except OSError as e:
                out[rel] = f"<unreadable: {e}>"
    return out


def diff(before, after):
    """A readable difference between two snapshots: (added, removed, changed)."""
    added = sorted(k for k in after if k not in before)
    removed = sorted(k for k in before if k not in after)
    changed = sorted(k for k in before if k in after and before[k] != after[k])
    return added, removed, changed


def describe(d):
    a, r, c = d
    bits = []
    if a:
        bits.append("added " + ", ".join(a))
    if r:
        bits.append("removed " + ", ".join(r))
    if c:
        bits.append("changed " + ", ".join(c))
    return "; ".join(bits) or "no change"


# ---------------------------------------------------------------------------- results

class Checks:
    """One flow's results. `same_tree` is the assertion most operations end with."""

    def __init__(self, name):
        self.name = name
        self.failures = []
        self.passes = 0

    def check(self, what, cond, detail=""):
        if cond:
            self.passes += 1
            print(f"  PASS {what}")
        else:
            self.failures.append(what)
            print(f"  FAIL {what}   {detail}")
        return bool(cond)

    def same_tree(self, what, before, after):
        d = diff(before, after)
        return self.check(what, not any(d), describe(d))

    def tree_changed(self, what, before, after, added=(), removed=()):
        a, r, c = diff(before, after)
        okk = sorted(a) == sorted(added) and sorted(r) == sorted(removed)
        return self.check(what, okk, describe((a, r, c)))
