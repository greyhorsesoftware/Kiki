"""Copy and move between every pair of ends: this machine, an SFTP server, an FTPS server.

Real servers, not mocks: a user-mode `sshd` (OpenSSH, its own host key, port and config, no
root) and a pyftpdlib FTPS server with a self-signed certificate. Both serve a folder of the
fixture, so every transfer is checked where it matters — on disk, byte for byte — and not by
asking the daemon what it thinks it did.

Nine pairs (three ends, each to each, same end included) × copy and move. A pair whose server
cannot be started here is skipped by name; with neither, the flow is.

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
    ends = {}
    local_root = os.path.join(base, "local")
    os.makedirs(local_root)
    ends["local"] = (local_root, lambda rel: "file://" + os.path.join(local_root, rel))
    try:
        if sshd_bin():
            sftp_root = os.path.join(base, "sftp-root")
            os.makedirs(sftp_root)
            port, key = servers.start_sftp()
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
    finally:
        for name in ("e2e-sftp", "e2e-ftps"):
            d.call("RemoveLocation", name=name)
        servers.stop()


def _job_state(d, job):
    d.drain(0.1)
    last = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job]
    return last[-1] if last else "no events"
