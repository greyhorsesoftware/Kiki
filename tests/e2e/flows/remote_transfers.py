"""Copy and move between every pair of ends: this machine, an SFTP server, an FTPS server.

Real servers, not mocks and not stand-ins: OpenSSH's `sshd` and `vsftpd` (over explicit TLS), each
run as the user running the tests with its own keys, port and config (`servers.py`). Both serve a
folder of the fixture, so every transfer is checked where it matters — on disk, byte for byte — and not by
asking the daemon what it thinks it did.

Nine pairs (three ends, each to each, same end included) × copy and move. A pair whose server
cannot be started here is skipped by name; with neither, the flow is.

Then, for each server: a transfer runs on a connection of its own (plan 31, phase 2). While a
large upload is running a folder never seen before still lists, the upload can be cancelled
mid-file without a half file left under its name, the panes' connection survives that, and when
the jobs are over the browser's connection is the only one the server still has.

    sudo pacman -S openssh vsftpd
"""
import getpass, os, shutil, subprocess, time
from harness import snapshot, wait_for
from servers import FTPS_PASSWORD, FTPS_USER, Servers, add_location, missing, sshd_bin, vsftpd_bin

NEEDS = {"daemon"}


def probe(ctx):
    """None to run; otherwise why not."""
    return missing()
TITLE = "copy and move between local, SFTP and FTPS"

HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
JOB_TIMEOUT = 90


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
    for probe in ("before", "during", "during-again", "after"):
        os.makedirs(os.path.join(root, f"probe-{probe}-{tag}"))
        open(os.path.join(root, f"probe-{probe}-{tag}", "here.txt"), "w").close()
    os.makedirs(os.path.join(root, f"inbox-{tag}"))
    c.check(f"{tag}: with every job over, none of their connections is left", connections(port) == 0, connections(port))
    lists(ctx, uri(f"probe-before-{tag}"), "here.txt")
    idle = connections(port)
    c.check(f"{tag}: browsing opens the one connection the panes use", idle == 1, idle)

    # Twice at most. On a loaded machine the listing can be slow enough (and loopback fast enough)
    # for the upload to be over by the time the folder has listed, and then the listing proved
    # nothing about running beside it. A listing that WAITS for the upload loses both times — it
    # comes back only once the upload is done — so the second go forgives the machine, not the bug.
    for probe in ("during", "during-again"):
        shutil.rmtree(os.path.join(root, f"inbox-{tag}")); os.makedirs(os.path.join(root, f"inbox-{tag}"))
        job = d.ok("Submit", op={"op": "copy", "items": [local_uri(f"big-{tag}.bin")], "dest": uri(f"inbox-{tag}")})["job"]
        moving = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and (e["job"]["bytes"] > 0 or e["job"]["state"] in ("done", "failed", "cancelled")), timeout=30)
        j = moving["job"] if moving else {}
        prepared = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] == "running"]
        busy = connections(port)
        took = lists(ctx, uri(f"probe-{probe}-{tag}"), "here.txt")
        still = _job_state(d, job)
        if isinstance(still, dict) and still["state"] == "running":
            break
        print(f"  NOTE {tag}: the upload was over before the folder had listed ({took and round(took, 2)} s) — once more")
        d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("done", "failed", "cancelled"), timeout=60)
    c.check(f"{tag}: the upload is under way", moving and moving["job"]["state"] == "running", moving and moving["job"])
    # What the activity view is told about it (plan 32).
    c.check(f"{tag}: it says what it is: an upload of that file, and which file is in hand",
            j.get("direction") == "upload" and j.get("name") == f"big-{tag}.bin" and (j.get("current") or {}).get("name") == f"big-{tag}.bin" and (j.get("current") or {}).get("size") == BIG_MB * 1024 * 1024,
            {k: j.get(k) for k in ("direction", "name", "current", "phase")})
    c.check(f"{tag}: it was 'preparing' until its totals were known, then 'running'", prepared and prepared[0].get("phase") == "preparing" and j.get("phase") == "running", [p.get("phase") for p in prepared][:4])
    c.check(f"{tag}: on a connection of its own", busy == idle + 1, f"{idle} before, {busy} during")
    c.check(f"{tag}: a folder never seen before lists while it runs", took is not None and took < 5, f"{took and round(took, 2)} s")
    c.check(f"{tag}: and the upload was still running when it did (else this proved nothing)", isinstance(still, dict) and still["state"] == "running", still)

    # Cancelled at once — loopback is fast, and every moment spent asking something else is a chance
    # for the upload to finish first. (How fast it goes is `transfer_remote`'s to check: a rate
    # needs half a second of bytes behind it, and this upload is not given that long.)
    d.events.clear()
    d.ok("Cancel", job=job)
    ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("cancelled", "done", "failed"), timeout=30)
    said = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"].get("cancelling")]
    if ended and ended["job"]["state"] == "done" and not said:
        # Said, not passed or failed: it was over before the cancel reached it. Nothing was tested.
        print(f"  NOTE {tag}: the upload finished before it could be cancelled — cancel not observed this run")
        os.remove(big)
        return
    c.check(f"{tag}: 'cancelling' is said at once, while it is still running", any(x["state"] == "running" for x in said), [(x["state"], x.get("cancelling")) for x in said][:3])
    c.check(f"{tag}: cancelled mid-file, it ends as cancelled", ended and ended["job"]["state"] == "cancelled", ended and ended["job"])
    c.check(f"{tag}: and leaves nothing behind: no half file under its name, no part file", os.listdir(os.path.join(root, f"inbox-{tag}")) == [], os.listdir(os.path.join(root, f"inbox-{tag}")))
    took = lists(ctx, uri(f"probe-after-{tag}"), "here.txt")
    c.check(f"{tag}: the panes' connection survived the cancel", took is not None, took)
    closed = wait_for(lambda: connections(port) == idle or None, timeout=5)
    c.check(f"{tag}: and the job's own connection is closed", closed, connections(port))
    os.remove(big)


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


def mirror(d, master, replica, direction):
    """Scan, read the plan's summary, run it. Returns (files to copy, the report's text)."""
    import re
    scan = d.submit({"op": "mirrorScan", "spec": {"master": master, "replica": replica, "direction": direction}})
    text = d.ok("MirrorReport", job=scan)["text"]
    n = int(re.search(r"Summary: (\d+) to copy", text).group(1))
    if n:
        d.wait_job(d.ok("Submit", op={"op": "mirrorRun", "plan": scan, "spec": {"master": master, "replica": replica, "direction": direction}})["job"], timeout=JOB_TIMEOUT)
    return n, text


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
        if vsftpd_bin():
            ftps_root = os.path.join(base, "ftps-root")
            os.makedirs(ftps_root)
            port = servers.start_ftps(ftps_root)
            ports["ftps"] = port
            r = add_location(d, {"name": "e2e-ftps", "plugin": "ftps", "remoteUri": "ftps://e2e-ftps" + ftps_root, "localUri": "",
                                 "config": {"host": "127.0.0.1", "port": str(port), "username": FTPS_USER, "encryption": "explicit"}}, {"password": FTPS_PASSWORD})
            if c.check("the FTPS location is added, its certificate trusted", "ok" in r and not r["ok"].get("verify"), r):
                # Full paths, as for SFTP: an unprivileged vsftpd cannot chroot, so its "/" is the
                # machine's, not the fixture's.
                ends["ftps"] = (ftps_root, lambda rel: "ftps://e2e-ftps" + os.path.join(ftps_root, rel))
        else:
            print("  (no vsftpd — sudo pacman -S vsftpd: the FTPS pairs are skipped)")

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

        # When part of it goes wrong (plan 31, phase 4). A file that cannot be read does not stop
        # the others; a move takes the original away only when ALL of it arrived and was checked;
        # and an original that cannot be removed is "copied, but…", not "failed".
        for tag in ports:
            if tag not in ends:
                continue
            r_root, r_uri = ends[tag]
            for op in ("copy", "move"):
                src = os.path.join(local_root, f"partial-{op}-{tag}", "site")
                build(src)
                os.chmod(os.path.join(src, "index.html"), 0)
                os.makedirs(os.path.join(r_root, f"partial-in-{op}-{tag}"))
                job = d.ok("Submit", op={"op": op, "items": [ends["local"][1](f"partial-{op}-{tag}/site")], "dest": r_uri(f"partial-in-{op}-{tag}")})["job"]
                ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("done", "failed", "cancelled"), timeout=JOB_TIMEOUT)
                err = (ended or {}).get("job", {}).get("error") or ""
                c.check(f"{tag} {op}: one unreadable file fails the job, by name and by count", ended and ended["job"]["state"] == "failed" and "1 of 6" in err and "site/index.html" in err, err)
                got = snapshot(os.path.join(r_root, f"partial-in-{op}-{tag}", "site"))
                c.check(f"{tag} {op}: and the other five arrived all the same", "images/logo.bin" in got and "images/2026/may.txt" in got and "index.html" not in got, sorted(got))
                os.chmod(os.path.join(src, "index.html"), 0o644)
                if op == "move":
                    c.check(f"{tag} move: a move with anything missing keeps the whole original", sorted(snapshot(src)) == sorted(k for k in snapshot(os.path.join(local_root, f"partial-copy-{tag}", "site"))), sorted(snapshot(src)))

            # An original that will not go: its folder on the server is read-only.
            os.makedirs(os.path.join(r_root, f"stuck-{tag}"))
            with open(os.path.join(r_root, f"stuck-{tag}", "report.txt"), "w") as f:
                f.write("all of it")
            os.chmod(os.path.join(r_root, f"stuck-{tag}"), 0o555)
            os.makedirs(os.path.join(local_root, f"unstuck-{tag}"))
            job = d.ok("Submit", op={"op": "move", "items": [r_uri(f"stuck-{tag}/report.txt")], "dest": ends["local"][1](f"unstuck-{tag}")})["job"]
            ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("done", "failed", "cancelled"), timeout=JOB_TIMEOUT)
            err = (ended or {}).get("job", {}).get("error") or ""
            os.chmod(os.path.join(r_root, f"stuck-{tag}"), 0o755)
            arrived = os.path.join(local_root, f"unstuck-{tag}", "report.txt")
            c.check(f"{tag}: a move whose original cannot be removed says 'copied, but…' — not that it failed to arrive", err.startswith("copied, but the original could not be removed: report.txt"), err)
            c.check(f"{tag}: and it did arrive, whole, with the original still there", os.path.exists(arrived) and open(arrived).read() == "all of it" and os.path.exists(os.path.join(r_root, f"stuck-{tag}", "report.txt")))

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
            # The wire is in it since 0.1.1 — over FTPS every command and reply of the control
            # channel, a dozen lines a file (TYPE, PASV, STOR, 150/226, RNFR/RNTO, MDTM): 177 for
            # these six. A packet dump would be thousands.
            c.check(f"{tag}: a story, not a packet dump", len(lines) < 400, len(lines))
            text = " ".join(l["text"] for l in lines) + " ".join(l["text"] for l in d.ok("LocationLog", location=f"e2e-{tag}")["lines"])
            c.check(f"{tag}: and the password is nowhere in it, nor in the location's own log", FTPS_PASSWORD not in text)
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

        # The other things a job does on a server (plan 31, phase 4): make a folder, rename, delete
        # a folder with things in it; a name that is taken; and what time the file says it is.
        for tag in ports:
            if tag not in ends:
                continue
            r_root, r_uri = ends[tag]
            work = os.path.join(r_root, f"ops-{tag}")
            os.makedirs(os.path.join(work, "doomed", "deep"))
            for rel, text in (("old.txt", "renamed"), ("doomed/a.txt", "a"), ("doomed/deep/b.txt", "b"), ("bystander.txt", "stays"), ("a.txt", "existing")):
                with open(os.path.join(work, rel), "w") as f:
                    f.write(text)

            d.submit({"op": "mkdir", "uri": r_uri(f"ops-{tag}/made")})
            c.check(f"{tag}: New Folder on the server makes it", os.path.isdir(os.path.join(work, "made")))
            d.submit({"op": "rename", "uri": r_uri(f"ops-{tag}/old.txt"), "name": "new name.txt"})
            c.check(f"{tag}: Rename renames it there", not os.path.exists(os.path.join(work, "old.txt")) and open(os.path.join(work, "new name.txt")).read() == "renamed", os.listdir(work))
            d.submit({"op": "delete", "items": [r_uri(f"ops-{tag}/doomed")]})
            c.check(f"{tag}: Delete removes a folder and everything in it", not os.path.exists(os.path.join(work, "doomed")), os.listdir(work))
            c.check(f"{tag}: and nothing beside it", open(os.path.join(work, "bystander.txt")).read() == "stays")

            # A name that is taken, answered each way.
            mine = os.path.join(local_root, f"clash-{tag}")
            os.makedirs(mine)
            with open(os.path.join(mine, "a.txt"), "w") as f:
                f.write("incoming")
            for answer, want in (("skip", {"a.txt": "existing"}), ("keepBoth", {"a.txt": "existing", "a (2).txt": "incoming"}), ("replace", {"a.txt": "incoming", "a (2).txt": "incoming"})):
                job = d.ok("Submit", op={"op": "copy", "items": [ends["local"][1](f"clash-{tag}/a.txt")], "dest": r_uri(f"ops-{tag}")})["job"]
                prompt = d.wait_event(lambda e: e.get("event") == "Prompt" and e.get("job") == job, timeout=20)
                c.check(f"{tag} {answer}: a name taken on the server is asked about, by name", prompt is not None and prompt["uri"].endswith("/a.txt"), prompt)
                if prompt:
                    d.ok("PromptReply", job=job, choice=answer, applyToAll=False)
                d.wait_job(job, timeout=JOB_TIMEOUT)
                got = {n: open(os.path.join(work, n)).read() for n in os.listdir(work) if n.startswith("a")}
                c.check(f"{tag} {answer}: and it does what was chosen — the same name a local copy would get", got == want, got)

            # What time it is. Up: only where the protocol can say (FTP cannot set a file's time).
            # Down: always — the file on this machine takes the time the server gives for it.
            old = 1_400_000_000
            dated = os.path.join(local_root, f"dated-{tag}.txt")
            with open(dated, "w") as f:
                f.write("from 2014")
            os.utime(dated, (old, old))
            os.makedirs(os.path.join(r_root, f"dated-in-{tag}"))
            d.submit({"op": "copy", "items": [ends["local"][1](f"dated-{tag}.txt")], "dest": r_uri(f"dated-in-{tag}")})
            up = os.path.getmtime(os.path.join(r_root, f"dated-in-{tag}", f"dated-{tag}.txt"))
            if tag == "sftp":
                c.check("sftp: an uploaded file keeps its time", abs(up - old) <= 2, up)
            else:
                c.check("ftps: an uploaded file is dated now — FTP has no way to set it, and kiki does not pretend", abs(up - time.time()) < 120, up)
            served = os.path.join(r_root, f"dated-in-{tag}", "served.txt")
            with open(served, "w") as f:
                f.write("from 2014, on the server")
            os.utime(served, (old, old))
            os.makedirs(os.path.join(local_root, f"dated-out-{tag}"))
            d.submit({"op": "copy", "items": [r_uri(f"dated-in-{tag}/served.txt")], "dest": ends["local"][1](f"dated-out-{tag}")})
            down = os.path.getmtime(os.path.join(local_root, f"dated-out-{tag}", "served.txt"))
            c.check(f"{tag}: a downloaded file takes the time the server gives it", abs(down - old) <= 2, down)

        # Ctrl+Z after a copy TO a server (plan 07). There is no trash on the other side, so the
        # inverse is a delete that cannot be taken back in its turn: it deletes what the copy made
        # and nothing else — not the folder it landed in, not a file that has changed since — and
        # both toasts say that it is for good. Checked on the server's own disk.
        for tag in ports:
            if tag not in ends:
                continue
            r_root, r_uri = ends[tag]
            src = os.path.join(local_root, f"undo-{tag}", "site")
            build(src)
            before = snapshot(src)
            inbox = os.path.join(r_root, f"undo-in-{tag}")
            os.makedirs(inbox)
            with open(os.path.join(inbox, "theirs.txt"), "w") as f:
                f.write("was here first")
            job = d.ok("Submit", op={"op": "copy", "items": [ends["local"][1](f"undo-{tag}/site")], "dest": r_uri(f"undo-in-{tag}")})["job"]
            d.wait_job(job, timeout=JOB_TIMEOUT)
            offered = d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == job, timeout=20)
            c.check(f"{tag}: the copy's toast says what its Undo would do, before it is clicked",
                    offered and f"Undo deletes it from e2e-{tag}, permanently" in offered["text"], offered and offered["text"])
            c.same_tree(f"{tag}: the copy arrived", before, snapshot(os.path.join(inbox, "site")))

            u = d.ok("Undo")["job"]
            said = d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == u, timeout=JOB_TIMEOUT)
            d.wait_job(u, timeout=JOB_TIMEOUT)
            c.check(f"{tag}: Undo takes it off the server's own disk", not os.path.exists(os.path.join(inbox, "site")), os.listdir(inbox))
            c.check(f"{tag}: and says how much went, from where, and that it is for good",
                    said and said["text"].startswith("Undid copy — 10 items deleted from e2e-") and said["text"].endswith("(permanently)"), said and said["text"])
            c.check(f"{tag}: what was in that folder before it is untouched", os.path.exists(os.path.join(inbox, "theirs.txt")), os.listdir(inbox))
            c.check(f"{tag}: and so is the original on this machine", snapshot(src) == before)

            # What somebody has changed since is not this copy's to delete. The check is the one
            # the mirror makes about the same server: SFTP keeps the time a file was given, so a
            # file rewritten at the same length is still seen to have changed; FTP cannot keep one,
            # and the size is all there is.
            job = d.ok("Submit", op={"op": "copy", "items": [ends["local"][1](f"undo-{tag}/site")], "dest": r_uri(f"undo-in-{tag}")})["job"]
            d.wait_job(job, timeout=JOB_TIMEOUT)
            d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == job, timeout=20)
            with open(os.path.join(inbox, "site", "index.html"), "a") as f:
                f.write("<!-- and a line of mine -->")
            same_length = os.path.join(inbox, "site", "üñí.txt")
            with open(same_length, "w") as f:
                f.write("UNICODE")  # "unicode" was seven bytes, and so is this
            os.utime(same_length, (time.time() + 5, time.time() + 5))
            u = d.ok("Undo")["job"]
            said = d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == u, timeout=JOB_TIMEOUT)
            d.wait_job(u, timeout=JOB_TIMEOUT)
            got = snapshot(os.path.join(inbox, "site"))
            c.check(f"{tag}: a file that grew since the copy is left where it is", got.get("index.html", b"").endswith(b"<!-- and a line of mine -->"), sorted(got))
            c.check(f"{tag}: and the rest of the copy is gone all the same", "images/logo.bin" not in got and "empty.txt" not in got, sorted(got))
            if tag == "sftp":
                # Two files and the folder it could not empty: three things are still there, and
                # the line says three.
                c.check("sftp: a file rewritten at the same length is left too — the time it carries says it is not the copy's",
                        got.get("üñí.txt") == b"UNICODE" and said and "3 items left because they had changed" in said["text"], said and said["text"])
            else:
                c.check("ftps: with no time to compare, one of the same length is taken for the copy's — all FTP can promise",
                        "üñí.txt" not in got and said and "2 items left because they had changed" in said["text"], said and said["text"])
            c.check(f"{tag}: the folder it could not empty stays; the ones it did empty are gone",
                    os.path.isdir(os.path.join(inbox, "site")) and not os.path.exists(os.path.join(inbox, "site", "images")) and not os.path.exists(os.path.join(inbox, "site", "hollow")), sorted(got))
            shutil.rmtree(os.path.join(inbox, "site"))

            # A move to a server offers nothing to undo: taking it back means putting an original
            # back, and there is nowhere on the other side to take it from.
            with open(os.path.join(local_root, f"undo-move-{tag}.txt"), "w") as f:
                f.write("moved")
            d.events.clear()
            job = d.ok("Submit", op={"op": "move", "items": [ends["local"][1](f"undo-move-{tag}.txt")], "dest": r_uri(f"undo-in-{tag}")})["job"]
            d.wait_job(job, timeout=JOB_TIMEOUT)
            d.drain(0.3)
            toasts = [e for e in d.events if e.get("event") == "Toast" and e.get("job") == job]
            c.check(f"{tag}: a move to the server offers no undo at all", toasts == [] and os.path.exists(os.path.join(inbox, f"undo-move-{tag}.txt")), toasts)

        # Server to server: the undo deletes on the destination, and only there.
        if "sftp" in ends and "ftps" in ends:
            src = os.path.join(ends["sftp"][0], "cross-undo", "site")
            build(src)
            before = snapshot(src)
            os.makedirs(os.path.join(ends["ftps"][0], "cross-undo-in"))
            job = d.ok("Submit", op={"op": "copy", "items": [ends["sftp"][1]("cross-undo/site")], "dest": ends["ftps"][1]("cross-undo-in")})["job"]
            d.wait_job(job, timeout=JOB_TIMEOUT)
            d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == job, timeout=20)
            c.same_tree("sftp → ftps: the copy arrives", before, snapshot(os.path.join(ends["ftps"][0], "cross-undo-in", "site")))
            u = d.ok("Undo")["job"]
            said = d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == u, timeout=JOB_TIMEOUT)
            d.wait_job(u, timeout=JOB_TIMEOUT)
            c.check("sftp → ftps: Undo deletes it from the FTPS server", not os.path.exists(os.path.join(ends["ftps"][0], "cross-undo-in", "site")), os.listdir(os.path.join(ends["ftps"][0], "cross-undo-in")))
            c.check("sftp → ftps: and says which server it went from", said and "deleted from e2e-ftps (permanently)" in said["text"], said and said["text"])
            c.same_tree("sftp → ftps: the SFTP side it was copied from is untouched", before, snapshot(src))

        # A mirror settles (plan 08; the rule is RelaySFTP's, which the engine is ported from): what
        # is copied is stamped with the time the NEXT SCAN will see, so a second run finds nothing
        # to do. The test of it is a file old enough that FTP's LIST gives only its day — if the
        # replica were stamped with anything truer than the listing, it would be copied for ever.
        for tag in ports:
            if tag not in ends:
                continue
            r_root, r_uri = ends[tag]
            for way in ("download", "upload"):
                here = os.path.join(local_root, f"mirror-{way}-{tag}")
                there = os.path.join(r_root, f"mirror-{way}-{tag}")
                os.makedirs(here); os.makedirs(there)
                src = there if way == "download" else here
                os.makedirs(os.path.join(src, "sub"))
                for n, (rel, age) in enumerate((("old.txt", 12 * 365 * 86400), ("sub/last-year.txt", 400 * 86400), ("sub/this-morning.txt", 3 * 3600 + 17), ("now.txt", 0))):
                    with open(os.path.join(src, rel), "w") as f:
                        f.write(f"file {n} " * (n + 3))
                    t = time.time() - age
                    os.utime(os.path.join(src, rel), (t, t))
                local_u, remote_u = ends["local"][1](f"mirror-{way}-{tag}"), r_uri(f"mirror-{way}-{tag}")
                master, replica = (remote_u, local_u) if way == "download" else (local_u, remote_u)
                first, _ = mirror(d, master, replica, way)
                c.check(f"{tag} mirror {way}: the first run copies the four files", first == 4, first)
                dst = here if way == "download" else there
                c.check(f"{tag} mirror {way}: and they arrive", sorted(k for k, v in snapshot(dst).items() if v != "d") == ["now.txt", "old.txt", "sub/last-year.txt", "sub/this-morning.txt"], sorted(snapshot(dst)))
                again, report = mirror(d, master, replica, way)
                c.check(f"{tag} mirror {way}: a second run finds nothing to do — old files, this morning's and this minute's alike", again == 0, report[-900:])

    finally:
        for name in ("e2e-sftp", "e2e-ftps"):
            d.call("RemoveLocation", name=name)
        servers.stop()


def _job_state(d, job):
    d.drain(0.1)
    last = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job]
    return last[-1] if last else "no events"
