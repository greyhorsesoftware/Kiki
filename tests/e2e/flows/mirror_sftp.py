"""Mirror to a real server, through the workspace (plans 08 and 29 A).

OpenSSH's `sshd` on a high port with its own keys and `authorized_keys` (`servers.py`), and
`vsftpd` over explicit TLS where it is installed. What this adds to `mirror_local`:

- **spread modification times.** A tree whose files are twelve years, a year, three hours and a
  moment old is uploaded, and the second run finds nothing to do — which is only true if each copy
  was stamped with the time the *next scan* will read.
- **a run cancelled while bytes are moving**, after which the browser still lists the folder it was
  writing into (the mirror's sessions are its own, and must be logged out cleanly even when the
  cancellation token is already set).
- **a compare cancelled while it is walking** a tree of twenty thousand files a side: the way out
  of the Preflight screen has to work on a server, which is the only place a compare takes long
  enough to want one.
- **one FTPS run** where `vsftpd` is installed, whose detector is size-only because FTP cannot set
  a modification time. Without it that half is skipped by name.

    sudo pacman -S openssh vsftpd
"""
import getpass, json, os, time
from harness import snapshot, wait_for
from servers import FTPS_PASSWORD, FTPS_USER, Servers, add_location, sshd_bin, vsftpd_bin
from flows.mirror_local import mirror, preflight, wait_ws, ws

NEEDS = {"shell"}
TITLE = "mirror to a server: times kept, a run cancelled, FTPS"


def probe(ctx):
    return None if sshd_bin() else "sshd is not installed (sudo pacman -S openssh)"


# Old enough that FTP's LIST gives only the day, this morning, and this minute: if a copy were
# stamped with anything but the time the next scan reads, one of these would be copied for ever.
AGES = (("old.txt", 12 * 365 * 86400), ("sub/last-year.txt", 400 * 86400), ("sub/this-morning.txt", 3 * 3600 + 17), ("now.txt", 0))


def spread(root):
    """Four files in two folders, their times spread over twelve years."""
    os.makedirs(os.path.join(root, "sub"), exist_ok=True)
    for n, (rel, age) in enumerate(AGES):
        with open(os.path.join(root, rel), "w") as f:
            f.write(f"file {n} " * (n + 3))
        t = time.time() - age
        os.utime(os.path.join(root, rel), (t, t))
    return root


def pair(ctx, local, remote_uri):
    """Both panes, then the workspace open on Configure over them."""
    sh = ctx.shell
    sh.call("split", "on")
    sh.call("focusPane", "left")
    sh.open("file://" + local)
    sh.call("focusPane", "right")
    sh.open(remote_uri)
    ws(sh, "open")
    return wait_ws(sh, lambda s: s.get("open") and s.get("screen") == "configure") or ws(sh)


def one_way(ctx, tag, local, remote_root, remote_uri, detector):
    """Upload a tree of spread times, then look for a second run to do: there must be none."""
    c, sh = ctx.checks, ctx.shell
    spread(local)
    st = pair(ctx, local, remote_uri)
    c.check(f"{tag}: the workspace opens over the local folder and the server's",
            st.get("local") == "file://" + local and st.get("remote") == remote_uri and st.get("direction") == "upload", st)
    st = preflight(sh)
    c.check(f"{tag}: the plan is the whole tree — four files and the folder they are in",
            st.get("screen") == "review" and st["counts"].get("new") == 5 and st["counts"].get("changed") == 0, st.get("counts"))
    ws(sh, "report")
    text = (wait_ws(sh, lambda s: s.get("report")) or ws(sh)).get("report", "")
    c.check(f"{tag}: and the report says which detector this server asked for", f"Detector:          {detector}" in text, text[:400])

    st = mirror(sh)
    c.check(f"{tag}: the run ends done, four files copied", st["run"]["state"] == "done" and (st["run"]["result"] or {}).get("copies") == 4, st.get("run"))
    c.check(f"{tag}: and says so", st.get("summary", "").startswith("Mirror complete · 4 copied · 0 deleted"), st.get("summary"))
    c.same_tree(f"{tag}: the server's folder is the local one, entry for entry", snapshot(local), snapshot(remote_root))

    ws(sh, "close")
    ws(sh, "open")
    st = preflight(sh)
    c.check(f"{tag}: a second run finds nothing to do — old files, this morning's and this minute's alike",
            (st["counts"].get("new"), st["counts"].get("changed"), st["counts"].get("equal")) == (0, 0, 5), st.get("counts"))
    ws(sh, "close")


def cancel_midway(ctx, tag, local, remote_root, remote_uri):
    """Cancel once bytes are moving: it says it was cancelled, leaves no half file under a final
    name, and the browser still lists the folder it was writing into."""
    c, sh = ctx.checks, ctx.shell
    block = os.urandom(1 << 20)
    os.makedirs(local, exist_ok=True)
    for i in range(24):
        with open(os.path.join(local, "big-%02d.bin" % i), "wb") as f:
            for _ in range(8):
                f.write(block)
    pair(ctx, local, remote_uri)
    st = preflight(sh)
    c.check(f"{tag}: 24 files are planned, 192 MB of them", st["counts"].get("new") == 24 and st["counts"].get("copyBytes") == 24 * 8 * (1 << 20), st.get("counts"))

    ws(sh, "run")
    moving = wait_ws(sh, lambda s: (s.get("run") or {}).get("bytes", 0) > 0 or (s.get("run") or {}).get("state") in ("done", "failed"), timeout=60)
    if (moving or {}).get("run", {}).get("state") in ("done", "failed"):
        # Said, not passed: a cancel that arrived after the event proves nothing.
        print(f"  NOTE {tag}: the upload was over before it could be cancelled — cancel not observed this run")
    else:
        ws(sh, "cancel")
        st = wait_ws(sh, lambda s: (s.get("run") or {}).get("state") in ("cancelled", "done", "failed"), timeout=60) or ws(sh)
        c.check(f"{tag}: cancelled mid-run, the run ends as cancelled", st["run"]["state"] == "cancelled", st.get("run"))
        c.check(f"{tag}: and the workspace says so, where it would have said what it copied", st.get("summary") == "Mirror cancelled", st.get("summary"))
    got = os.listdir(remote_root)
    short = [n for n in got if not n.endswith(".kiki-part") and os.path.getsize(os.path.join(remote_root, n)) != 8 * (1 << 20)]
    c.check(f"{tag}: nothing is left half-written under its own name, and no part file behind", not short and not [n for n in got if n.endswith(".kiki-part")], got[:5])

    # The browser: the pane that is still standing in that folder on the server.
    ws(sh, "close")
    sh.call("focusPane", "right")
    sh.call("open", remote_uri)
    sh.wait_state(lambda s: s.get("uri") == remote_uri and s.get("done"))
    st = sh.state()
    c.check(f"{tag}: and the folder it was writing into still lists after the cancel",
            st.get("error") == "" and st.get("count") == len(got), (st.get("count"), len(got), st.get("error")))


def big_tree(root, files, prefix="d", dirs=100):
    """A tree too big to compare in a blink, written straight onto the disk the server serves."""
    os.makedirs(root, exist_ok=True)
    for d in range(dirs):
        p = os.path.join(root, "%s%03d" % (prefix, d))
        os.makedirs(p, exist_ok=True)
        for f in range(files // dirs):
            with open(os.path.join(p, "f%04d.txt" % f), "w") as fh:
                fh.write("x")
    return root


def scan_job(sh):
    """The daemon's own view of the last compare. It is a hidden job — machinery, not something
    the user asked for — so `activity` is where it shows rather than the activity view."""
    try:
        jobs = json.loads(sh.call("activity") or "[]")
    except json.JSONDecodeError:
        return None
    scans = [j for j in jobs if j.get("op") == "mirrorScan"]
    return scans[-1] if scans else None


def cancel_the_compare(ctx, tag, local, remote_root, remote_uri):
    """The owner's report: "need ability to cancel the mirror compare". A compare of twenty
    thousand files a side is cancelled while it is walking — and must come straight back to the
    form with nothing left over, end as cancelled in the daemon, leave the pane able to list the
    server, and let the next compare run to the end."""
    c, sh = ctx.checks, ctx.shell
    big_tree(local, 20000)
    # Named apart from the local side's, so the second compare has a whole tree to find.
    big_tree(remote_root, 20000, prefix="r")
    pair(ctx, local, remote_uri)

    ws(sh, "preflight")
    # Cancel while it is really walking, not while it is still connecting: the screen counts the
    # entries the daemon has seen, so waiting for that count to move is waiting for the walk.
    was = wait_ws(sh, lambda s: s.get("scanning", 0) > 0 or s.get("screen") != "preflight", timeout=60) or ws(sh)
    walking = was.get("screen") == "preflight"
    c.check(f"{tag}: the compare says what it is doing and how far it has got",
            not walking or ("Comparing" in was.get("status", "") and "items so far" in was.get("status", "")), (was.get("screen"), was.get("status")))

    started = time.time()
    st = ws(sh, "cancel")
    back = time.time() - started
    c.check(f"{tag}: Cancel comes straight back to the form", st.get("screen") == "configure" and back < 2, (st.get("screen"), round(back, 2)))
    c.check(f"{tag}: and the form says the compare was cancelled, not that it is still comparing",
            st.get("status") == "Compare cancelled" and st.get("statusError") is False, (st.get("status"), st.get("statusError")))

    # The daemon's own end of it: the job stops, and stops as cancelled. It used to go on walking
    # the tree — and to go on holding the server's connection — with nobody left to tell.
    job = wait_for(lambda: (lambda j: j if j and j.get("state") not in ("running", "queued") else None)(scan_job(sh)), timeout=5)
    ended = time.time() - started
    if walking:
        c.check(f"{tag}: and the daemon's compare ends as cancelled, at once", (job or {}).get("state") == "cancelled" and ended < 3, (job or scan_job(sh), round(ended, 2)))
    else:
        # Said, not passed: a cancel that arrived after the compare was over proves nothing.
        print(f"  NOTE {tag}: the compare was over before it could be cancelled — cancel not observed this run")

    # The pane standing in that folder on the server: the compare's sessions were its own and
    # were logged out cleanly, cancellation token set or not (plan 06).
    sh.call("focusPane", "right")
    sh.call("open", remote_uri)
    sh.wait_state(lambda s: s.get("uri") == remote_uri and s.get("done"))
    state = sh.state()
    c.check(f"{tag}: the folder it was comparing still lists after the cancel", state.get("error") == "" and state.get("count") == 100, (state.get("count"), state.get("error")))

    # And a second compare, asked for straight away, runs to the end.
    ws(sh, "preflight")
    st = wait_ws(sh, lambda s: s.get("screen") == "review", timeout=120) or ws(sh)
    c.check(f"{tag}: a compare asked for again works, and finds the whole tree", st.get("screen") == "review" and st["counts"].get("new") == 20100, (st.get("screen"), st.get("counts")))
    ws(sh, "close")


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    base = ctx.base
    os.makedirs(base, exist_ok=True)
    servers = Servers(base)
    local = os.path.join(base, "local")
    os.makedirs(local)
    try:
        sftp_root = os.path.join(base, "sftp-root")
        os.makedirs(sftp_root)
        port, key = servers.start_sftp()
        r = add_location(d, {"name": "e2e-mirror-sftp", "plugin": "sftp", "remoteUri": "sftp://e2e-mirror-sftp" + sftp_root, "localUri": "",
                             "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
        if not c.check("the SFTP location is added, its host key trusted", "ok" in r and not r["ok"].get("verify"), r):
            return
        uri = lambda rel: "sftp://e2e-mirror-sftp" + os.path.join(sftp_root, rel)

        os.makedirs(os.path.join(sftp_root, "spread"))
        one_way(ctx, "sftp", os.path.join(local, "spread"), os.path.join(sftp_root, "spread"), uri("spread"), "size+mtime")

        os.makedirs(os.path.join(sftp_root, "big"))
        cancel_midway(ctx, "sftp", os.path.join(local, "big"), os.path.join(sftp_root, "big"), uri("big"))

        os.makedirs(os.path.join(sftp_root, "many"))
        cancel_the_compare(ctx, "sftp", os.path.join(local, "many"), os.path.join(sftp_root, "many"), uri("many"))

        # FTP cannot set a modification time, so its detector is the size alone — which is what
        # keeps an FTPS upload idempotent, and what the report must show.
        if not vsftpd_bin():
            print("  (no vsftpd — sudo pacman -S vsftpd: the FTPS run is skipped)")
            return
        ftps_root = os.path.join(base, "ftps-root")
        os.makedirs(ftps_root)
        fport = servers.start_ftps(ftps_root)
        r = add_location(d, {"name": "e2e-mirror-ftps", "plugin": "ftps", "remoteUri": "ftps://e2e-mirror-ftps" + ftps_root, "localUri": "",
                             "config": {"host": "127.0.0.1", "port": str(fport), "username": FTPS_USER, "encryption": "Explicit TLS (AUTH TLS)"}}, {"password": FTPS_PASSWORD})
        if not c.check("the FTPS location is added, its certificate trusted", "ok" in r and not r["ok"].get("verify"), r):
            return
        os.makedirs(os.path.join(ftps_root, "spread"))
        one_way(ctx, "ftps", os.path.join(local, "ftps-spread"), os.path.join(ftps_root, "spread"),
                "ftps://e2e-mirror-ftps" + os.path.join(ftps_root, "spread"), "size only")
    finally:
        ctx.shell.call("mirror", "close")
        for name in ("e2e-mirror-sftp", "e2e-mirror-ftps"):
            d.call("RemoveLocation", name=name)
        servers.stop()
