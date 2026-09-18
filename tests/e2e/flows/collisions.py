"""A destination that already exists: each answer to the prompt has to produce its own tree.

This is the case that quietly loses data when it goes wrong, so every branch is checked against
the bytes on disk, not the job's own report.
"""
from harness import snapshot, wait_for

NEEDS = {"daemon"}
TITLE = "collisions: skip, replace, keep both"


def paste(ctx, answer):
    """Copy src/a.txt over dst/a.txt and answer the prompt with `answer`."""
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"src": {"a.txt": "incoming"}, "dst": {"a.txt": "existing"}})
    src, dst = root + "/src", root + "/dst"
    job = d.ok("Submit", op={"op": "copy", "items": ["file://" + src + "/a.txt"], "dest": "file://" + dst})["job"]
    prompt = d.wait_event(lambda e: e.get("event") == "Prompt" and e.get("job") == job)
    c.check(f"the {answer} run raises a collision prompt", prompt is not None)
    if prompt:
        c.check("it names the file that is in the way", prompt["uri"].endswith("/a.txt"), prompt.get("uri"))
        d.ok("PromptReply", job=job, choice=answer, applyToAll=False)
    d.wait_job(job)
    return dst, snapshot(dst)


def run(ctx):
    c = ctx.checks

    dst, after = paste(ctx, "skip")
    c.check("skip leaves the existing file untouched", after.get("a.txt") == b"existing", after)
    c.check("and adds nothing", list(after) == ["a.txt"], list(after))

    dst, after = paste(ctx, "replace")
    c.check("replace writes the incoming bytes", after.get("a.txt") == b"incoming", after)
    c.check("and still leaves one file", list(after) == ["a.txt"], list(after))

    dst, after = paste(ctx, "keepBoth")
    c.check("keep both keeps the original as it was", after.get("a.txt") == b"existing", after)
    kept = [k for k in after if k != "a.txt"]
    c.check("and lands the incoming one beside it", len(kept) == 1 and after[kept[0]] == b"incoming", after)
