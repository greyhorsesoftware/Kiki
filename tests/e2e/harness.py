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

    def select(self, name):
        self.call("select", name)
        return self.wait_state(lambda s: any(u.endswith("/" + name) for u in s.get("selection", [])))

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
