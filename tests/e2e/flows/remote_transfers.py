"""Copy and move between every pair of ends: this machine, an SFTP server, an FTPS server.

Real servers, not mocks: a user-mode `sshd` (OpenSSH, its own host key, port and config, no
root) and a pyftpdlib FTPS server with a self-signed certificate. Both serve a folder of the
fixture, so every transfer is checked where it matters — on disk, byte for byte — and not by
asking the daemon what it thinks it did.

Nine pairs (three ends, each to each, same end included) × copy and move. A pair whose server
cannot be started here is skipped by name; with neither, the flow is.

Then, for each server: a transfer runs on a connection of its own (plan 31, phase 2). While a
large upload is running a folder never seen before still lists, the upload can be cancelled
mid-file without a half file left under its name, the panes' connection survives that, and when
the jobs are over the browser's connection is the only one the server still has.

    make e2e-servers        # once: pyftpdlib + pyOpenSSL into tests/e2e/.venv
"""
import getpass, os, shutil, socket, subprocess, time
from harness import snapshot, wait_for

NEEDS = {"daemon"}
TITLE = "copy and move between local, SFTP and FTPS"

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JOB_TIMEOUT = 90


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def ftps_python():
    """A python that can import pyftpdlib and OpenSSL, or None."""
    for py in [os.environ.get("KIKI_E2E_FTPS_PYTHON"), os.path.join(HERE, ".venv", "bin", "python"), shutil.which("python3")]:
        if py and os.path.exists(py) and subprocess.run([py, "-c", "import pyftpdlib, OpenSSL"], capture_output=True).returncode == 0:
            return py
    return None


def sshd_bin():
    return shutil.which("sshd") or next((p for p in ["/usr/bin/sshd", "/usr/sbin/sshd"] if os.path.exists(p)), None)


def probe(ctx):
    if not sshd_bin() and not ftps_python():
        return "no sshd, and no pyftpdlib (make e2e-servers)"
    return None


class Servers:
    def __init__(self, base):
        self.base = base
        self.procs = []
        self.sshd_pid = None

    def start_sftp(self):
        d = os.path.join(self.base, "sshd")
        os.makedirs(d)
        for name in ("host_key", "client_key"):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", os.path.join(d, name)], check=True)
        shutil.copy(os.path.join(d, "client_key.pub"), os.path.join(d, "authorized_keys"))
        os.chmod(os.path.join(d, "authorized_keys"), 0o600)
        port = free_port()
        with open(os.path.join(d, "sshd_config"), "w") as f:
            f.write(f"Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/host_key\nPidFile {d}/sshd.pid\n"
                    f"AuthorizedKeysFile {d}/authorized_keys\nPasswordAuthentication no\nKbdInteractiveAuthentication no\n"
                    "UsePAM no\nStrictModes no\nSubsystem sftp internal-sftp\n")
        subprocess.run([sshd_bin(), "-f", os.path.join(d, "sshd_config"), "-E", os.path.join(d, "sshd.log")], check=True)
        pid = wait_for(lambda: os.path.exists(f"{d}/sshd.pid") and open(f"{d}/sshd.pid").read().strip(), what="sshd up")
        self.sshd_pid = int(pid) if pid else None
        return port, os.path.join(d, "client_key")

    def start_ftps(self, py, root):
        d = os.path.join(self.base, "ftpsd")
        os.makedirs(d)
        cert, key = os.path.join(d, "cert.pem"), os.path.join(d, "key.pem")
        subprocess.run(["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-days", "2",
                        "-subj", "/CN=localhost", "-addext", "subjectAltName=IP:127.0.0.1", "-keyout", key, "-out", cert], check=True, capture_output=True)
        port = free_port()
        p = subprocess.Popen([py, os.path.join(HERE, "ftps_server.py"), root, str(port), cert, key, "kiki", "s3cret"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        self.procs.append(p)
        line = p.stdout.readline()
        if "READY" not in line:
            raise RuntimeError("ftps server did not start: " + line + p.stdout.read())
        return port

    def stop(self):
        for p in self.procs:
            p.terminate()
        if self.sshd_pid:
            try:
                os.kill(self.sshd_pid, 15)
            except ProcessLookupError:
                pass


def connections(port):
    """Established TCP connections to a server's (control) port, counted from the server's side."""
    out = subprocess.run(["ss", "-Htn", "state", "established", f"( sport = :{port} )"], capture_output=True, text=True).stdout
    return len([l for l in out.splitlines() if l.strip()])


def lists(ctx, uri, name, timeout=8):
    """How long a folder the daemon has never listed takes to show `name`, or None."""
    d, lid, t0 = ctx.daemon, ctx.lid(), time.time()
    if "ok" not in d.call("Open", lid=lid, uri=uri):
        return None
    got = wait_for(lambda: any(r["name"] == name for r in d.ok("Window", lid=lid, first=0, count=60)["rows"]) or None, timeout=timeout)
    d.call("Close", lid=lid)
    return (time.time() - t0) if got else None


# Big enough that it is still going after everything asked of it below (loopback is fast).
BIG_MB = 1536


def own_connection(ctx, c, d, tag, port, root, uri, local_root, local_uri):
    """A long upload beside the browser: see the module docstring."""
    big = os.path.join(local_root, f"big-{tag}.bin")
    block = os.urandom(4 * 1024 * 1024)
    with open(big, "wb") as f:
        for _ in range(BIG_MB // 4):
            f.write(block)
    for probe in ("before", "during", "after"):
        os.makedirs(os.path.join(root, f"probe-{probe}-{tag}"))
        open(os.path.join(root, f"probe-{probe}-{tag}", "here.txt"), "w").close()
    os.makedirs(os.path.join(root, f"inbox-{tag}"))
    c.check(f"{tag}: with every job over, none of their connections is left", connections(port) == 0, connections(port))
    lists(ctx, uri(f"probe-before-{tag}"), "here.txt")
    idle = connections(port)
    c.check(f"{tag}: browsing opens the one connection the panes use", idle == 1, idle)

    job = d.ok("Submit", op={"op": "copy", "items": [local_uri(f"big-{tag}.bin")], "dest": uri(f"inbox-{tag}")})["job"]
    moving = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and (e["job"]["bytes"] > 0 or e["job"]["state"] in ("done", "failed", "cancelled")), timeout=30)
    c.check(f"{tag}: the upload is under way", moving and moving["job"]["state"] == "running", moving and moving["job"])
    # What the activity view is told about it (plan 32).
    j = moving["job"] if moving else {}
    c.check(f"{tag}: it says what it is: an upload of that file, and which file is in hand",
            j.get("direction") == "upload" and j.get("name") == f"big-{tag}.bin" and (j.get("current") or {}).get("name") == f"big-{tag}.bin" and (j.get("current") or {}).get("size") == BIG_MB * 1024 * 1024,
            {k: j.get(k) for k in ("direction", "name", "current", "phase")})
    prepared = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] == "running"]
    c.check(f"{tag}: it was 'preparing' until its totals were known, then 'running'", prepared and prepared[0].get("phase") == "preparing" and j.get("phase") == "running", [p.get("phase") for p in prepared][:4])
    busy = connections(port)
    c.check(f"{tag}: on a connection of its own", busy == idle + 1, f"{idle} before, {busy} during")
    took = lists(ctx, uri(f"probe-during-{tag}"), "here.txt")
    still = _job_state(d, job)
    c.check(f"{tag}: a folder never seen before lists while it runs", took is not None and took < 5, f"{took and round(took, 2)} s")
    c.check(f"{tag}: and the upload was still running when it did (else this proved nothing)", isinstance(still, dict) and still["state"] == "running", still)

    fast = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"].get("rate", 0) > 0, timeout=3)
    c.check(f"{tag}: and how fast it is going", fast is not None, _job_state(d, job))
    d.events.clear()
    d.ok("Cancel", job=job)
    ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("cancelled", "done", "failed"), timeout=30)
    said = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"].get("cancelling")]
    c.check(f"{tag}: 'cancelling' is said at once, while it is still running", any(x["state"] == "running" for x in said), [(x["state"], x.get("cancelling")) for x in said][:3])
    c.check(f"{tag}: cancelled mid-file, it ends as cancelled", ended and ended["job"]["state"] == "cancelled", ended and ended["job"])
    c.check(f"{tag}: and leaves nothing behind: no half file under its name, no part file", os.listdir(os.path.join(root, f"inbox-{tag}")) == [], os.listdir(os.path.join(root, f"inbox-{tag}")))
    took = lists(ctx, uri(f"probe-after-{tag}"), "here.txt")
    c.check(f"{tag}: the panes' connection survived the cancel", took is not None, took)
    closed = wait_for(lambda: connections(port) == idle or None, timeout=5)
    c.check(f"{tag}: and the job's own connection is closed", closed, connections(port))
    os.remove(big)


def add_location(d, location, secrets):
    """Add it the way the dialog does: the first answer is the server's key, the second trusts it."""
    r = d.call("AddLocation", location=location, secrets=secrets)
    if "ok" in r and r["ok"].get("verify"):
        r = d.call("AddLocation", location=location, secrets=secrets, trust=r["ok"]["verify"])
    return r


TREE = {
    "index.html": "<html>kiki</html>",
    "name with spaces.txt": "spaces",
    "üñí.txt": "unicode",
    "empty.txt": "",
    "images": {"logo.bin": "BIG", "2026": {"may.txt": "deep"}},
    "hollow": {},
}


def build(root):
    """TREE under `root`; logo.bin is 3 MB of bytes that do not compress or repeat."""
    def put(at, spec):
        os.makedirs(at, exist_ok=True)
        for name, v in spec.items():
            if isinstance(v, dict):
                put(os.path.join(at, name), v)
            else:
                with open(os.path.join(at, name), "wb") as f:
                    f.write(os.urandom(3 * 1024 * 1024) if v == "BIG" else v.encode())
    put(root, TREE)


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    base = ctx.base
    os.makedirs(base, exist_ok=True)
    servers = Servers(base)
    # end -> (folder on this disk, function from a path under it to the URI the daemon is given)
    ends, ports = {}, {}
    local_root = os.path.join(base, "local")
    os.makedirs(local_root)
    ends["local"] = (local_root, lambda rel: "file://" + os.path.join(local_root, rel))
    try:
        if sshd_bin():
            sftp_root = os.path.join(base, "sftp-root")
            os.makedirs(sftp_root)
            port, key = servers.start_sftp()
            ports["sftp"] = port
            r = add_location(d, {"name": "e2e-sftp", "plugin": "sftp", "remoteUri": "sftp://e2e-sftp" + sftp_root, "localUri": "",
                                 "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
            if c.check("the SFTP location is added, its host key trusted", "ok" in r and not r["ok"].get("verify"), r):
                ends["sftp"] = (sftp_root, lambda rel: "sftp://e2e-sftp" + os.path.join(sftp_root, rel))
        else:
            print("  (no sshd: the SFTP pairs are skipped)")
        py = ftps_python()
        if py:
            ftps_root = os.path.join(base, "ftps-root")
            os.makedirs(ftps_root)
            port = servers.start_ftps(py, ftps_root)
            ports["ftps"] = port
            r = add_location(d, {"name": "e2e-ftps", "plugin": "ftps", "remoteUri": "ftps://e2e-ftps/", "localUri": "",
                                 "config": {"host": "127.0.0.1", "port": str(port), "username": "kiki", "encryption": "Explicit TLS (AUTH TLS)"}}, {"password": "s3cret"})
            if c.check("the FTPS location is added, its certificate trusted", "ok" in r and not r["ok"].get("verify"), r):
                ends["ftps"] = (ftps_root, lambda rel: "ftps://e2e-ftps/" + rel)
        else:
            print("  (no pyftpdlib — make e2e-servers: the FTPS pairs are skipped)")

        n = 0
        for src in ends:
            for dst in ends:
                for op in ("copy", "move"):
                    n += 1
                    tag = f"{op} {src} -> {dst}"
                    s_root, s_uri = ends[src]
                    d_root, d_uri = ends[dst]
                    s_rel, d_rel = f"from-{n}", f"to-{n}"
                    build(os.path.join(s_root, s_rel, "site"))
                    os.makedirs(os.path.join(d_root, d_rel))
                    before = snapshot(os.path.join(s_root, s_rel, "site"))

                    job = d.ok("Submit", op={"op": op, "items": [s_uri(s_rel + "/site")], "dest": d_uri(d_rel)})["job"]
                    done = d.wait_job(job, timeout=JOB_TIMEOUT)
                    if not c.check(f"{tag}: the job finishes", done is not None, _job_state(d, job)):
                        continue
                    arrived = snapshot(os.path.join(d_root, d_rel, "site"))
                    c.same_tree(f"{tag}: every file and folder arrives, byte for byte", before, arrived)
                    left = os.path.exists(os.path.join(s_root, s_rel, "site"))
                    if op == "copy":
                        c.check(f"{tag}: and the original is untouched", left and snapshot(os.path.join(s_root, s_rel, "site")) == before)
                    else:
                        c.check(f"{tag}: and nothing is left behind", not left, os.listdir(os.path.join(s_root, s_rel)))
        c.check("every pair of ends that could be started was tried", n == 2 * len(ends) ** 2, n)

        # The job's log (plan 32): kiki's own account, and beneath it what the plugin and its
        # library did on the server — for that job, with no secret in it.
        for tag in ports:
            if tag not in ends:
                continue
            build(os.path.join(local_root, f"log-{tag}", "site"))
            os.makedirs(os.path.join(ends[tag][0], f"log-in-{tag}"))
            job = d.ok("Submit", op={"op": "copy", "items": [ends["local"][1](f"log-{tag}/site")], "dest": ends[tag][1](f"log-in-{tag}")})["job"]
            d.wait_job(job, timeout=JOB_TIMEOUT)
            log = d.ok("JobLog", job=job)
            lines = log["lines"]
            mine = [l["text"] for l in lines if l["source"] == "kiki"]
            theirs = [l for l in lines if l["source"].startswith(tag)]
            c.check(f"{tag}: the log opens with what was asked and closes with how it went", mine and "started" in mine[0] and mine[-1].startswith("finished: 6 of 6 items"), mine[:1] + mine[-1:])
            c.check(f"{tag}: it names each file as it is begun", any(t.startswith("site/images/logo.bin (3145728 bytes)") for t in mine), mine)
            c.check(f"{tag}: and holds what was done on the server for it: the part file written, then renamed into place",
                    any("logo.bin.kiki-part" in l["text"] and l["text"].startswith("write ") for l in theirs) and any(l["text"].startswith("rename ") and "logo.bin" in l["text"] for l in theirs), [l["text"][:60] for l in theirs][:6])
            c.check(f"{tag}: on a session of that job's own", any(f"(job-{job})" in l["text"] for l in theirs), [l["text"] for l in theirs if "connect" in l["text"]])
            c.check(f"{tag}: a story, not a packet dump", len(lines) < 150, len(lines))
            text = " ".join(l["text"] for l in lines) + " ".join(l["text"] for l in d.ok("LocationLog", location=f"e2e-{tag}")["lines"])
            c.check(f"{tag}: and the password is nowhere in it, nor in the location's own log", "s3cret" not in text)
            more = d.ok("JobLog", job=job, **{"from": log["next"]})
            c.check(f"{tag}: reading on from where the last read ended gets nothing twice", more["lines"] == [] and more["next"] == log["next"], more)
            d.ok("DismissJob", job=job)
            c.check(f"{tag}: a dismissed job's log goes with it", "err" in d.call("JobLog", job=job))

        for tag, port in ports.items():
            if tag in ends:
                own_connection(ctx, c, d, tag, port, ends[tag][0], ends[tag][1], local_root, ends["local"][1])

        # Searching a location finds what is deep inside it — on FTPS too, whose plugin has no
        # recursive scan and answers Unsupported, which used to be the end of the search.
        for tag in ports:
            if tag not in ends:
                continue
            lid = ctx.lid()
            r = d.call("Search", lid=lid, scope="location", uri=ends[tag][1](""), query="may.txt")
            rows = d.ok("Window", lid=lid, first=0, count=60)["rows"] if "ok" in r else []
            deep = [x["uri"] for x in rows if x["uri"].endswith("/site/images/2026/may.txt")]
            c.check(f"{tag}: a search of the location finds a file three folders down", "ok" in r and len(deep) >= 1, r if "ok" not in r else f"{len(rows)} rows")
            d.call("Close", lid=lid)
    finally:
        for name in ("e2e-sftp", "e2e-ftps"):
            d.call("RemoveLocation", name=name)
        servers.stop()


def _job_state(d, job):
    d.drain(0.1)
    last = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job]
    return last[-1] if last else "no events"
