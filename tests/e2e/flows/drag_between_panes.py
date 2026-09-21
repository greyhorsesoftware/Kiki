"""Dragging between the panes, without a pointer.

`shell drop <uris> <dest> <modifiers>` (the URIs newline-separated, as a `text/uri-list`) hands `Pane.dropInto` an event shaped as Qt
shapes a real one — which has no modifiers, only the keys folded into `proposedAction` — so
everything a real drag does after the button goes down is what runs here: what the keys choose,
which drops are refused, and the job that comes of it. Real drags, keys and all, are driven in
`tests/qml-drag` under Qt's `minimal` platform; only the compositor's part stays on the phase 10
checklist.

Every pair of ends is covered — local, SFTP and FTPS, in both directions and each to itself — so
that "move within one place, copy between places" is asserted where it actually differs. A server
that is not installed skips its own pairs by name and the rest still run.
"""
import getpass
import json
import os

from harness import wait_for
from servers import FTPS_PASSWORD, FTPS_USER, Servers, add_location, sshd_bin, vsftpd_bin

NEEDS = {"shell"}
TITLE = "dropping between the panes: every pair and modifier"


def drop(sh, uris, dest, mods=""):
    """What the drop decided: {accepted, action, items}. Empty when the shell said nothing.

    The URIs go as a newline-separated list, the shape of a `text/uri-list`: Quickshell's IPC
    strips square brackets out of an argument, so JSON cannot be handed to it."""
    try:
        return json.loads(sh.call("drop", "\n".join(uris), dest, mods) or "{}")
    except json.JSONDecodeError:
        return {}


def question(sh, answer=""):
    """What the yes/no dialog is asking — {open, title, message, label, danger} — answering it
    first with "yes" or "no" when one is given."""
    try:
        return json.loads(sh.call("question", answer) or "{}")
    except json.JSONDecodeError:
        return {}


def failures(d):
    """What the daemon says went wrong lately — a drop is a job, and a job that fails says why."""
    try:
        jobs = d.ok("Jobs").get("jobs", [])
    except Exception:
        return ""
    bad = [f"{j.get('op')}: {j.get('error')}" for j in jobs if j.get("error")]
    return "; ".join(bad[-3:])


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    base = ctx.base
    os.makedirs(base, exist_ok=True)
    servers = Servers(base)

    # end -> (the folder on this disk, a function from a name under it to the URI kiki is given)
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
        if vsftpd_bin():
            ftps_root = os.path.join(base, "ftps-root")
            os.makedirs(ftps_root)
            port = servers.start_ftps(ftps_root)
            r = add_location(d, {"name": "e2e-ftps", "plugin": "ftps", "remoteUri": "ftps://e2e-ftps" + ftps_root, "localUri": "",
                                 "config": {"host": "127.0.0.1", "port": str(port), "username": FTPS_USER, "encryption": "Explicit TLS (AUTH TLS)"}}, {"password": FTPS_PASSWORD})
            if c.check("the FTPS location is added, its certificate trusted", "ok" in r and not r["ok"].get("verify"), r):
                ends["ftps"] = (ftps_root, lambda rel: "ftps://e2e-ftps" + os.path.join(ftps_root, rel))
        else:
            print("  (no vsftpd — sudo pacman -S vsftpd: the FTPS pairs are skipped)")

        sh.open("file://" + local_root)

        n = 0
        for src in ends:
            for dst in ends:
                n += 1
                sdisk, suri = ends[src]
                ddisk, duri = ends[dst]
                pair = f"{src} -> {dst}"
                # A folder of its own for each pair, so one pair cannot see another's leavings.
                sdir, ddir = os.path.join(sdisk, f"from-{n}"), os.path.join(ddisk, f"into-{n}")
                os.makedirs(sdir)
                os.makedirs(ddir)

                # ---------------------------------------------------- what no modifier does
                # Within one place a drag moves; between two places it copies, because a move
                # between machines is a copy and then a delete and nobody asked for the delete.
                open(os.path.join(sdir, "plain.txt"), "w").write("plain")
                want = "move" if src == dst else "copy"
                r = drop(sh, [suri(f"from-{n}/plain.txt")], duri(f"into-{n}"))
                c.check(f"{pair}: a plain drop {want}s", r.get("action") == want and r.get("accepted") is True, r)
                landed = wait_for(lambda: os.path.exists(os.path.join(ddir, "plain.txt")) or None, timeout=60)
                c.check(f"{pair}: …and the file is there", landed is not None, failures(d) or os.listdir(ddir))
                if want == "move":
                    gone = wait_for(lambda: (not os.path.exists(os.path.join(sdir, "plain.txt"))) or None, timeout=60)
                    c.check(f"{pair}: …and gone from where it was", gone is not None, os.listdir(sdir))

                # ---------------------------------------------------- Ctrl copies, always
                open(os.path.join(sdir, "ctrl.txt"), "w").write("ctrl")
                r = drop(sh, [suri(f"from-{n}/ctrl.txt")], duri(f"into-{n}"), "ctrl")
                c.check(f"{pair}: Ctrl copies", r.get("action") == "copy", r)
                got = wait_for(lambda: os.path.exists(os.path.join(ddir, "ctrl.txt")) or None, timeout=60)
                c.check(f"{pair}: …the copy arrives and the original stays",
                        got is not None and os.path.exists(os.path.join(sdir, "ctrl.txt")), os.listdir(sdir))

                # ---------------------------------------------------- Shift reads as no key
                # A real drag carries no modifiers, only `proposedAction`, and Qt reports Shift
                # exactly as it reports no key at all: so Shift follows the rule by place, and
                # cannot force a move between machines (cut and paste does that).
                open(os.path.join(sdir, "shift.txt"), "w").write("shift")
                r = drop(sh, [suri(f"from-{n}/shift.txt")], duri(f"into-{n}"), "shift")
                c.check(f"{pair}: Shift {want}s, as no key does", r.get("action") == want, r)
                arrived = wait_for(lambda: os.path.exists(os.path.join(ddir, "shift.txt")) or None, timeout=60)
                kept = os.path.exists(os.path.join(sdir, "shift.txt"))
                c.check(f"{pair}: …and the original is {'taken away' if want == 'move' else 'kept'}",
                        arrived is not None and (kept if want == "copy" else wait_for(lambda: (not os.path.exists(os.path.join(sdir, "shift.txt"))) or None, timeout=60) is not None),
                        (os.listdir(sdir), os.listdir(ddir)))

                # ---------------------------------------------------- several at once
                for i in range(3):
                    open(os.path.join(sdir, f"many{i}.txt"), "w").write(str(i))
                r = drop(sh, [suri(f"from-{n}/many{i}.txt") for i in range(3)], duri(f"into-{n}"), "ctrl")
                c.check(f"{pair}: a multiple selection is one drop", r.get("accepted") is True, r)
                all_there = wait_for(lambda: all(os.path.exists(os.path.join(ddir, f"many{i}.txt")) for i in range(3)) or None, timeout=60)
                c.check(f"{pair}: …and all of them land", all_there is not None, os.listdir(ddir))

        # -------------------------------------------------------------- onto the Trash
        # This machine's files go to the trash without a word. A server has no trash, so its
        # files are deleted for good — but only after a question that says so, in danger colours.
        def trashed(name):
            return any(i["path"].endswith("/" + name) for i in d.ok("TrashInfo")["items"])

        for end, (disk, uri) in ends.items():
            n += 1
            folder = os.path.join(disk, f"bin-{n}")
            os.makedirs(folder)
            name = f"binned-{n}.txt"
            open(os.path.join(folder, name), "w").write("bin")
            there = lambda: os.path.exists(os.path.join(folder, name))
            if end == "local":
                r = drop(sh, [uri(f"bin-{n}/{name}")], "trash:///")
                c.check("local -> Trash: taken, and nothing asked", r.get("accepted") is True and not question(sh).get("open"), r)
                c.check("…the file leaves its folder", wait_for(lambda: (not there()) or None, timeout=60) is not None, failures(d))
                c.check("…and is in the trash", wait_for(lambda: trashed(name) or None) is not None)
                continue
            r = drop(sh, [uri(f"bin-{n}/{name}")], "trash:///")
            q = question(sh)
            c.check(f"{end} -> Trash: it asks first, in danger colours",
                    r.get("accepted") is True and q.get("open") is True and q.get("danger") is True
                    and q.get("title") == "Delete permanently?" and "cannot be undone" in q.get("message", ""), (r, q))
            question(sh, "no")
            c.check(f"{end} -> Trash: …No leaves the file where it is",
                    not question(sh).get("open") and wait_for(lambda: not there() or None, timeout=2) is None and there(), os.listdir(folder))
            drop(sh, [uri(f"bin-{n}/{name}")], "trash:///")
            question(sh, "yes")
            c.check(f"{end} -> Trash: …Yes deletes it on the server",
                    wait_for(lambda: (not there()) or None, timeout=60) is not None, failures(d) or os.listdir(folder))
            c.check(f"{end} -> Trash: …and nothing went to this machine's trash", not trashed(name))

            # Del is the same rule: the question, then gone from the server. In a folder of its
            # own, made before kiki is told to look: a server has no watcher, so a file written
            # to its disk behind kiki's back is not seen in a folder already listed.
            here = os.path.join(disk, f"del-{n}")
            os.makedirs(here)
            open(os.path.join(here, "del.txt"), "w").write("del")
            sh.open(uri(f"del-{n}"))
            if not c.check(f"{end}: Del — the file is selected", sh.select("del.txt") is not None, sh.state()):
                continue
            sh.call("action", "trash")
            q = question(sh, "yes")
            c.check(f"{end}: Del asks first", q.get("open") is True and q.get("title") == "Delete permanently?", q)
            c.check(f"{end}: …and Yes deletes it on the server",
                    wait_for(lambda: (not os.path.exists(os.path.join(here, "del.txt"))) or None, timeout=60) is not None,
                    failures(d) or os.listdir(here))
        sh.open("file://" + local_root)

        # -------------------------------------------------------------- focus follows the drop
        # Side by side, a drop into the pane that does not have the focus hands it the focus:
        # that is where the files now are.
        was_split = sh.state().get("split") is True
        if not was_split:
            sh.call("sideBySide", "toggle")
        if c.check("side by side for the focus check", sh.wait_state(lambda st: st.get("split") is True) is not None, sh.state().get("split")):
            sh.call("focusPane", "left")
            here = os.path.join(local_root, "focus")
            os.makedirs(os.path.join(here, "into"))
            open(os.path.join(here, "f.txt"), "w").write("f")
            try:
                r = json.loads(sh.call("dropOn", "right", "file://" + here + "/f.txt", "file://" + here + "/into", "") or "{}")
            except json.JSONDecodeError:
                r = {}
            c.check("the pane a drop lands in takes the focus", r.get("accepted") is True and r.get("focused") == "right", r)
            sh.call("focusPane", "left")
        if not was_split:
            sh.call("sideBySide", "toggle")

        # -------------------------------------------------------------- what is refused
        # These need no server: they are `Pane.dropAction` saying no, and nothing is submitted.
        here = os.path.join(local_root, "refuse")
        os.makedirs(os.path.join(here, "inner"))
        open(os.path.join(here, "a.txt"), "w").write("a")
        huri = "file://" + here
        for what, uris, dest in [
            ("a file into the folder it is already in", [huri + "/a.txt"], huri),
            ("a folder onto itself", [huri + "/inner"], huri + "/inner"),
            ("a folder into something inside it", [huri], huri + "/inner"),
        ]:
            r = drop(sh, uris, dest)
            c.check(f"refused: {what}", r.get("accepted") is False and r.get("action") == "none", r)
        c.check("…and nothing was moved by any of them", sorted(os.listdir(here)) == ["a.txt", "inner"], os.listdir(here))
    finally:
        servers.stop()
