"""The activity orb and its popup, in the running window against the running daemon (plan 32).

The QML tests prove the view against made-up jobs; this proves the two halves meet — that what the
daemon really sends is what the view really says.
"""
import json, os
from harness import wait_for

NEEDS = {"daemon", "shell"}
TITLE = "the activity orb and what it opens"


def view(sh, action="state"):
    return json.loads(sh.call("activityView", action) or "{}")


def run(ctx):
    c, d, sh = ctx.checks, ctx.daemon, ctx.shell
    root = ctx.fixture({"site": {"a.txt": "a", "b.txt": "bb", "img": {"c.bin": "c" * 3000}}, "dst": {}, "notes.txt": "n"})
    d.ok("ClearJobs")
    v = wait_for(lambda: (lambda s: s if s.get("orb") == "idle" and s.get("entries") == [] else None)(view(sh)))
    c.check("with nothing going on the orb is idle and says so", v is not None and v["tip"] == "No activity", view(sh))

    job = d.ok("Submit", op={"op": "copy", "items": ["file://" + root + "/site"], "dest": "file://" + root + "/dst"})["job"]
    d.wait_job(job)
    v = wait_for(lambda: (lambda s: s if any(e["id"] == job and e["state"] == "done" for e in s.get("entries", [])) else None)(view(sh, "open")))
    mine = next((e for e in (v or {}).get("entries", []) if e["id"] == job), None)
    c.check("a folder copy is listed by the folder's name, and ends on what it did", mine and mine["headline"] == "site" and mine["line"] == "Copied 3 items", mine)

    # One step and gone: a trash that went well is not history…
    t = d.ok("Submit", op={"op": "trash", "items": ["file://" + root + "/notes.txt"]})["job"]
    d.wait_job(t)
    v = wait_for(lambda: (lambda s: s if not any(e["id"] == t for e in s["entries"]) else None)(view(sh)))
    c.check("a trash that went well leaves no entry behind", v is not None, view(sh)["entries"])
    # …and nor is the machinery of an undo.
    d.call("Undo")
    wait_for(lambda: os.path.exists(root + "/notes.txt") or None)
    c.check("nor does the undo that put it back", len(view(sh)["entries"]) == 1, view(sh)["entries"])

    # A failure turns the orb red until somebody has looked.
    sh.call("activityView", "close")
    bad = d.ok("Submit", op={"op": "copy", "items": ["file://" + root + "/never-was.txt"], "dest": "file://" + root + "/dst"})["job"]
    d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == bad and e["job"]["state"] == "failed")
    v = wait_for(lambda: (lambda s: s if s.get("orb") == "failed" else None)(view(sh)))
    c.check("a failed job turns the orb red", v is not None and v["tip"].startswith("1 failed"), view(sh))
    v = view(sh, "open")
    failed = next((e for e in v["entries"] if e["id"] == bad), None)
    c.check("opened, it is seen: the orb settles and the entry says what went wrong", v["orb"] == "idle" and failed and failed["state"] == "failed" and failed["line"], v)

    # A picture for whoever wants to see what the tests can only describe (kept in the out dir).
    import shutil, subprocess, time
    if shutil.which("grim"):
        time.sleep(0.4)
        subprocess.run(["grim", os.path.join(os.environ.get("KIKI_E2E_OUT", "/tmp"), "activity-popup.png")], capture_output=True)

    d.drain(0.3)
    c.check("a job that made nothing offers nothing to undo", not any(e.get("event") == "Toast" and e.get("job") == bad for e in d.events), [e for e in d.events if e.get("event") == "Toast"])
    c.check("and the error names the file, in words", failed and failed["line"].startswith("never-was.txt: not found"), failed)

    log = d.ok("JobLog", job=bad)["lines"]
    c.check("and its log says so too", any(l["level"] == "error" and l["text"].startswith("failed:") for l in log), [l["text"] for l in log])
    c.check("a failed job's log is kept on disk for the morning after", "failed-jobs.log" in os.listdir(os.environ["KIKI_STATE_DIR"]) and f"==== job {bad}:" in open(os.path.join(os.environ["KIKI_STATE_DIR"], "failed-jobs.log")).read())

    v = view(sh, "clear")
    v = wait_for(lambda: (lambda s: s if s.get("entries") == [] else None)(view(sh)))
    c.check("Clear empties the list, and it stays empty when the daemon is asked again", v is not None and d.ok("Jobs")["jobs"] == [], d.ok("Jobs")["jobs"])
    sh.call("activityView", "close")
