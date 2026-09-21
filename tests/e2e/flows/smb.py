"""An SMB share, end to end (plans 25 and 29 B).

A real `smbd` on a high port as the user running the tests (`servers.py`), reached the way kiki
reaches every share: through the `gvfsd`/`gvfsd-smb` that `run.sh` starts for the daemon on a
session bus of its own, so a run neither uses nor disturbs the developer's own gvfs mounts.

Plan 29 B's list — list, copy out, copy in, rename, delete, mkdir, mtime kept, mirror second run
empty, wrong password, disconnect while open — plus what is particular to a share:

- the location is added with its password going to the keyring, and browses in a pane;
- **Delete asks first, and says the file will not be in the trash**, because a server has none;
- **Undo of a copy to the server** deletes what the copy made and nothing else, which is only safe
  because SMB keeps a modification time (`pick_detector` says `sizeMtime` for it, and the times
  here prove it earns that);
- a wrong password is refused **at once**, in words, and does not hang the pane it was opened from;
- the server is killed under an open folder: the pane says so and comes back;
- and a second server that offers only SMB1 is refused by name.

    sudo pacman -S samba gvfs-smb
"""
import json, os, re, time
from harness import snapshot, wait_for
from servers import SMB_GUEST_SHARE, SMB_PASSWORD, SMB_SHARE, Servers, add_location, gvfsd_bin, gvfsd_smb_bin, pdbedit_bin, smb_user, smbd_bin

NEEDS = {"shell"}
TITLE = "an SMB share: browsed, transferred, mirrored, and refused"

JOB_TIMEOUT = 90
LOCATION = "e2e-smb"


def probe(ctx):
    """None to run; otherwise why not."""
    if not smbd_bin() or not pdbedit_bin():
        return "samba is not installed (sudo pacman -S samba)"
    if not gvfsd_bin() or not gvfsd_smb_bin():
        return "gvfs and its SMB backend are not installed (sudo pacman -S gvfs gvfs-smb)"
    smb = next((p for p in ctx.daemon.ok("Plugins")["plugins"] if p.get("scheme") == "smb"), None)
    if not smb:
        return "this build has no smb plugin"
    if not smb.get("available", True):
        # run.sh gives the daemon a bus and a gvfsd of its own; without one this is all it can say.
        return smb.get("unavailableReason") or "the smb plugin says it is not available"
    return None


TREE = {"index.html": "<html>kiki</html>", "name with spaces.txt": "spaces", "üñí.txt": "unicode", "empty.txt": "", "sub": {"deep.txt": "deep"}}


def build(root, spec=None):
    os.makedirs(root, exist_ok=True)
    for name, v in (TREE if spec is None else spec).items():
        if isinstance(v, dict):
            build(os.path.join(root, name), v)
        else:
            with open(os.path.join(root, name), "w") as f:
                f.write(v)
    return root


def mirror(d, master, replica, direction):
    """Scan, read the plan's summary, run it. Returns (files to copy, the report's text)."""
    scan = d.submit({"op": "mirrorScan", "spec": {"master": master, "replica": replica, "direction": direction}})
    text = d.ok("MirrorReport", job=scan)["text"]
    n = int(re.search(r"Summary: (\d+) to copy", text).group(1))
    if n:
        d.wait_job(d.ok("Submit", op={"op": "mirrorRun", "plan": scan, "spec": {"master": master, "replica": replica, "direction": direction}})["job"], timeout=JOB_TIMEOUT)
    return n, text


def ran(d, spec):
    """Submit an operation and hand back what went wrong, or "" — a job that failed silently is
    a check that passes for the wrong reason."""
    job = d.ok("Submit", op=spec)["job"]
    ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("done", "failed", "cancelled"), timeout=JOB_TIMEOUT)
    if not ended:
        return "the job never ended"
    return ended["job"].get("error") or ("" if ended["job"]["state"] == "done" else ended["job"]["state"])


def add(d, name, port, share, auth="password", secrets=None, password=SMB_PASSWORD):
    """The location the dialog would save: the port travels in the Host field, which is what
    gvfsd-smb is given and what it honours — there is no Port field in the SMB form (plan 25)."""
    return add_location(d, {"name": name, "plugin": "smb", "remoteUri": f"smb://{name}/", "localUri": "",
                            "config": {"name": name, "host": f"127.0.0.1:{port}", "share": share, "auth": auth,
                                       "username": "" if auth == "guest" else smb_user(), "domain": ""}},
                        {"password": password} if secrets is None else secrets)


def run(ctx):
    c, d, sh = ctx.checks, ctx.daemon, ctx.shell
    base = ctx.base
    os.makedirs(base, exist_ok=True)
    servers = Servers(base)
    share_root, guest_root = os.path.join(base, "share"), os.path.join(base, "guest")
    local = os.path.join(base, "local")
    for p in (share_root, guest_root, local):
        os.makedirs(p)
    try:
        port = servers.start_smb(share_root, guest_root)
        uri = lambda rel="": "smb://" + LOCATION + ("/" + rel if rel else "/")

        # ------------------------------------------------ signing in
        # A password that is not the password. This used to be the end of the run: gvfsd-smb asks
        # again after every refusal, the plugin answered every time, and `AddLocation` never came
        # back — the daemon gave up on it two minutes later with no idea why.
        t0 = time.time()
        r = add(d, LOCATION, port, SMB_SHARE, password="not the password")
        took = time.time() - t0
        c.check("a wrong password is refused, and says so in words a person can act on",
                "err" in r and "refused" in (r["err"].get("message") or ""), r)
        c.check("and it is refused at once, not eventually", took < 30, f"{took:.0f} s")

        r = add(d, LOCATION, port, SMB_SHARE)
        if not c.check("the SMB location is added", "ok" in r, r):
            return
        c.check("and its password went to the keyring, not into the config",
                SMB_PASSWORD not in open(os.path.join(os.environ["KIKI_CONFIG_DIR"], "locations.toml")).read())

        # A share that is not there is the share field's problem, named — kiki used to pass on
        # "No such file or directory" about a Windows share nobody had mentioned.
        r = add(d, "e2e-smb-gone", port, "nosuchshare")
        c.check("a share that is not there names the share, and the field it was typed in",
                "err" in r and "share" in (r["err"].get("message") or "") and "nosuchshare" in (r["err"].get("message") or ""), r)

        # ------------------------------------------------ browsing it in a pane
        build(os.path.join(share_root, "site"))
        os.makedirs(os.path.join(share_root, "empty-folder"))
        sh.open(uri())
        sh.wait_state(lambda s: s.get("uri") == uri() and s.get("done"))
        st = sh.state()
        c.check("the share opens in a pane, with what is on it", st.get("error") == "" and st.get("count") == 2, st)

        # The DOS Hidden attribute, which this server keeps in the other-execute bit: hidden on a
        # share is not a leading dot, and the pane's hidden filter has to honour the flag (plan 25).
        with open(os.path.join(share_root, "notes.txt"), "w") as f:
            f.write("hidden on the server")
        os.chmod(os.path.join(share_root, "notes.txt"), 0o645)
        sh.call("action", "refresh")
        sh.wait_state(lambda s: s.get("done") and s.get("count") == 2)
        c.check("a file with the DOS Hidden attribute is out of sight, without a dot in its name", sh.state().get("count") == 2, sh.state())
        sh.call("action", "hidden")
        shown = sh.wait_state(lambda s: s.get("done") and s.get("count") == 3)
        c.check("and Show hidden brings it back", shown is not None, sh.state())
        sh.call("action", "hidden")

        # ------------------------------------------------ copy out, copy in
        before = snapshot(os.path.join(share_root, "site"))
        job = d.ok("Submit", op={"op": "copy", "items": [uri("site")], "dest": "file://" + local})["job"]
        c.check("copy out: the job finishes", d.wait_job(job, timeout=JOB_TIMEOUT) is not None, _last(d, job))
        c.same_tree("copy out: every file arrives on this machine, byte for byte", before, snapshot(os.path.join(local, "site")))

        build(os.path.join(local, "mine"))
        os.makedirs(os.path.join(share_root, "inbox"))
        job = d.ok("Submit", op={"op": "copy", "items": ["file://" + os.path.join(local, "mine")], "dest": uri("inbox")})["job"]
        c.check("copy in: the job finishes", d.wait_job(job, timeout=JOB_TIMEOUT) is not None, _last(d, job))
        c.same_tree("copy in: and every file lands on the share", snapshot(os.path.join(local, "mine")), snapshot(os.path.join(share_root, "inbox", "mine")))

        # What time the file says it is. SMB keeps one and kiki can set one, which is why its
        # detector is size+mtime — the claim `pick_detector` makes and undo and mirror lean on.
        old = 1_400_000_000
        dated = os.path.join(local, "dated.txt")
        with open(dated, "w") as f:
            f.write("from 2014")
        os.utime(dated, (old, old))
        os.makedirs(os.path.join(share_root, "dated"))
        c.check("a file copied into a folder on the share arrives", ran(d, {"op": "copy", "items": ["file://" + dated], "dest": uri("dated")}) == "")
        up = os.path.getmtime(os.path.join(share_root, "dated", "dated.txt"))
        c.check("an uploaded file keeps its time — which is what lets SMB be mirrored on size and time", abs(up - old) <= 2, up)

        # ------------------------------------------------ mkdir, rename, delete
        why = ran(d, {"op": "mkdir", "uri": uri("made")})
        c.check("New Folder on the share makes it", why == "" and os.path.isdir(os.path.join(share_root, "made")), why or os.listdir(share_root))
        why = ran(d, {"op": "rename", "uri": uri("made"), "name": "renamed folder"})
        c.check("Rename renames it there", why == "" and not os.path.exists(os.path.join(share_root, "made")) and os.path.isdir(os.path.join(share_root, "renamed folder")), why or os.listdir(share_root))

        # Delete, through the window, with the question it has to ask: there is no trash on a
        # server, so the only delete a share has is the one that cannot be taken back.
        sh.open(uri())
        sh.wait_state(lambda s: s.get("uri") == uri() and s.get("done"))
        c.check("the folder to delete is selected", sh.select("renamed folder") is not None, sh.state().get("selection"))
        sh.call("action", "deleteForever")
        asked = sh.wait_state(lambda s: s.get("dialogs", {}).get("confirm"))
        c.check("Delete on a share asks first", asked is not None, sh.state())
        was = json.loads(sh.call("question", "yes") or "{}")
        c.check("and the question says it is for good — there is no trash on a server",
                "permanently" in (was.get("message", "") + was.get("title", "")).lower(), was)
        c.check("answering yes takes it off the share", wait_for(lambda: not os.path.exists(os.path.join(share_root, "renamed folder"))) is not None, os.listdir(share_root))

        # ------------------------------------------------ a mirror that settles
        for way in ("upload", "download"):
            here, there = os.path.join(local, f"mirror-{way}"), os.path.join(share_root, f"mirror-{way}")
            os.makedirs(here)
            os.makedirs(there)
            src = there if way == "download" else here
            os.makedirs(os.path.join(src, "sub"))
            for n, (rel, age) in enumerate((("old.txt", 12 * 365 * 86400), ("sub/last-year.txt", 400 * 86400), ("sub/this-morning.txt", 3 * 3600 + 17), ("now.txt", 0))):
                with open(os.path.join(src, rel), "w") as f:
                    f.write(f"file {n} " * (n + 3))
                t = time.time() - age
                os.utime(os.path.join(src, rel), (t, t))
            local_u, remote_u = "file://" + here, uri(f"mirror-{way}")
            master, replica = (remote_u, local_u) if way == "download" else (local_u, remote_u)
            first, _ = mirror(d, master, replica, way)
            c.check(f"mirror {way}: the first run copies the four files", first == 4, first)
            dst = here if way == "download" else there
            c.check(f"mirror {way}: and they arrive", sorted(k for k, v in snapshot(dst).items() if v != "d") == ["now.txt", "old.txt", "sub/last-year.txt", "sub/this-morning.txt"], sorted(snapshot(dst)))
            again, report = mirror(d, master, replica, way)
            c.check(f"mirror {way}: a second run finds nothing to do — old files, this morning's and this minute's alike", again == 0, report[-600:])
            c.check(f"mirror {way}: and the report says the share was compared on size and time", "Detector:          size+mtime" in report, report[:400])

        # ------------------------------------------------ Undo of a copy to the server
        # There is no trash on the other side, so the inverse of a copy is a delete that cannot be
        # taken back in its turn: it takes what the copy made and nothing else, and both toasts
        # say it is for good. Checked on the server's own disk.
        src = build(os.path.join(local, "undo", "site"))
        inbox = os.path.join(share_root, "undo-in")
        os.makedirs(inbox)
        with open(os.path.join(inbox, "theirs.txt"), "w") as f:
            f.write("was here first")
        job = d.ok("Submit", op={"op": "copy", "items": ["file://" + src], "dest": uri("undo-in")})["job"]
        d.wait_job(job, timeout=JOB_TIMEOUT)
        offered = d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == job, timeout=20)
        c.check("the copy's toast says what its Undo would do, before it is clicked",
                offered and f"Undo deletes it from {LOCATION}, permanently" in offered["text"], offered and offered["text"])
        u = d.ok("Undo")["job"]
        said = d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == u, timeout=JOB_TIMEOUT)
        d.wait_job(u, timeout=JOB_TIMEOUT)
        c.check("Undo takes the copy off the share's own disk", not os.path.exists(os.path.join(inbox, "site")), os.listdir(inbox))
        c.check("and says how much went, from where, and that it is for good",
                said and said["text"].startswith(f"Undid copy — 7 items deleted from {LOCATION}") and said["text"].endswith("(permanently)"), said and said["text"])
        c.check("what was in that folder before the copy is untouched", os.path.exists(os.path.join(inbox, "theirs.txt")), os.listdir(inbox))

        # And what somebody changed on the share since is not this copy's to delete. On SMB the
        # time a file carries is real, so a file rewritten at the same length is still seen to
        # have changed — which is more than FTP can promise about the same test.
        job = d.ok("Submit", op={"op": "copy", "items": ["file://" + src], "dest": uri("undo-in")})["job"]
        d.wait_job(job, timeout=JOB_TIMEOUT)
        d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == job, timeout=20)
        same_length = os.path.join(inbox, "site", "üñí.txt")
        with open(same_length, "w") as f:
            f.write("UNICODE")     # "unicode" was seven bytes, and so is this
        os.utime(same_length, (time.time() + 5, time.time() + 5))
        u = d.ok("Undo")["job"]
        said = d.wait_event(lambda e: e.get("event") == "Toast" and e.get("job") == u, timeout=JOB_TIMEOUT)
        d.wait_job(u, timeout=JOB_TIMEOUT)
        got = snapshot(os.path.join(inbox, "site"))
        c.check("a file rewritten on the share since the copy is left where it is — its time says it is not the copy's",
                got.get("üñí.txt") == b"UNICODE" and said and "2 items left because they had changed" in said["text"], said and said["text"])
        c.check("and the rest of the copy is gone all the same", "index.html" not in got and "empty.txt" not in got, sorted(got))

        # ------------------------------------------------ the guest share
        with open(os.path.join(guest_root, "open.txt"), "w") as f:
            f.write("anyone")
        r = add(d, "e2e-smb-guest", port, SMB_GUEST_SHARE, auth="guest", secrets={})
        if c.check("a guest share is added with no credentials at all", "ok" in r, r):
            sh.open("smb://e2e-smb-guest/")
            sh.wait_state(lambda s: s.get("uri") == "smb://e2e-smb-guest/" and s.get("done"))
            st = sh.state()
            c.check("and browses", st.get("error") == "" and st.get("count") == 1, st)

        # ------------------------------------------------ a server that offers only SMB1
        # Plan 25 wrote this refusal from Samba's source and never met a server. What gvfsd-smb
        # really hands over is an errno ("Software caused connection abort"); the words below are
        # kiki's, and this is the first time anything has checked they are said.
        nt1 = servers.start_smb(share_root, guest_root, tag="smbd-nt1", protocol="NT1")
        r = add(d, "e2e-smb1", nt1, SMB_SHARE)
        c.check("a server that offers only SMB1 is refused by name, not with a status code",
                "err" in r and "This server only offers SMB1, which kiki does not support." in (r["err"].get("message") or ""), r)

        # ------------------------------------------------ the server goes away under an open folder
        sh.open(uri("inbox"))
        sh.wait_state(lambda s: s.get("uri") == uri("inbox") and s.get("done"))
        servers.stop()
        t0 = time.time()
        sh.call("action", "refresh")
        sh.wait_state(lambda s: s.get("error") != "" or (s.get("done") and s.get("count") == 0), timeout=45)
        st = sh.state()
        c.check("with the server gone, the open folder says so rather than hanging", st.get("error") != "", st)
        c.check("and says it at once", time.time() - t0 < 40, f"{time.time() - t0:.0f} s")
        # And the window is still a window: another folder opens straight away.
        sh.open("file://" + local)
        c.check("the pane comes back — a dead server does not take the window with it",
                sh.wait_state(lambda s: s.get("uri") == "file://" + local and s.get("done")) is not None, sh.state())
    finally:
        for name in (LOCATION, "e2e-smb-gone", "e2e-smb-guest", "e2e-smb1"):
            d.call("RemoveLocation", name=name)
        servers.stop()
        # Nothing of this flow's is still running: `smbd` starts `samba-dcerpcd` and two `rpcd_*`
        # workers that are not in its process group, and `stop()` is what takes them with it.
        left = servers.strays()
        c.check("and no server of this flow's is left running", not left, [cmd[:70] for _, cmd in left])


def _last(d, job):
    d.drain(0.1)
    seen = [e["job"] for e in d.events if e.get("event") == "JobEvent" and e["job"]["id"] == job]
    return seen[-1] if seen else "no events"
