"""The cases that break file managers, each asserted on the tree it leaves behind."""
import os
from harness import snapshot, wait_for

NEEDS = {"daemon"}
TITLE = "edge cases"


def run(ctx):
    c, d = ctx.checks, ctx.daemon

    # A symlink is moved as a link, never followed into its target.
    root = ctx.fixture({"src": {"real.txt": "real"}, "dst": {}})
    os.symlink(root + "/src/real.txt", root + "/src/link.txt")
    d.submit({"op": "move", "items": ["file://" + root + "/src/link.txt"], "dest": "file://" + root + "/dst"})
    moved = root + "/dst/link.txt"
    c.check("a symlink moves as a link", os.path.islink(moved), os.path.realpath(moved) if os.path.exists(moved) else "missing")
    c.check("its target is left where it was", os.path.exists(root + "/src/real.txt"))

    # A destination that cannot be written fails the job and changes nothing.
    root = ctx.fixture({"src": {"a.txt": "alpha"}, "ro": {}})
    os.chmod(root + "/ro", 0o500)
    before = snapshot(root + "/src")
    job = d.ok("Submit", op={"op": "copy", "items": ["file://" + root + "/src/a.txt"], "dest": "file://" + root + "/ro"})["job"]
    ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("failed", "done"))
    c.check("a read-only destination fails the job", ended and ended["job"]["state"] == "failed", ended["job"] if ended else "no event")
    c.same_tree("and leaves the source alone", before, snapshot(root + "/src"))
    os.chmod(root + "/ro", 0o700)

    # Pasting into the folder the file is already in must not touch it.
    root = ctx.fixture({"here": {"a.txt": "alpha"}})
    before = snapshot(root + "/here")
    job = d.ok("Submit", op={"op": "copy", "items": ["file://" + root + "/here/a.txt"], "dest": "file://" + root + "/here"})["job"]
    d.wait_event(lambda e: e.get("event") == "Prompt" and e.get("job") == job, timeout=1.5)
    d.call("PromptReply", job=job, choice="keepBoth", applyToAll=False)
    d.wait_job(job)
    after = snapshot(root + "/here")
    c.check("a copy into its own folder keeps the original", after.get("a.txt") == b"alpha", after)

    # Cancelling a long copy stops it and leaves no half file where the job was aimed.
    root = ctx.fixture({"big": {f"f{i}.bin": 200_000 for i in range(60)}, "out": {}})
    job = d.ok("Submit", op={"op": "copy", "items": ["file://" + root + "/big"], "dest": "file://" + root + "/out"})["job"]
    d.ok("Cancel", job=job)
    ended = d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == job and e["job"]["state"] in ("cancelled", "done", "failed"))
    c.check("a cancelled copy ends as cancelled", ended and ended["job"]["state"] == "cancelled", ended["job"]["state"] if ended else "no event")
    c.check("the source is untouched by the cancellation", len(snapshot(root + "/big")) == 60)
