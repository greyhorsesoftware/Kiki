"""A large folder, this machine to this machine: many thousands of files eight folders deep, a few
very large ones, and every awkward name (plan 31, phase 4). Not in the default run — it takes a
minute at `transfer-lite` and a good while at `transfer`:

    tests/e2e/run.sh --flow transfer_local
    KIKI_TRANSFER_PROFILE=transfer tests/e2e/run.sh --flow transfer_local
"""
import os
from harness import wait_for
from transfer_common import generate, run_transfer, cancel_midway, walk

NEEDS = {"daemon"}
TITLE = "a large transfer, local to local"


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    base = ctx.base
    src = os.path.join(base, "big")
    os.makedirs(base, exist_ok=True)
    manifest = generate(src)
    print(f"  fixture: {manifest['files']} files in {manifest['folders']} folders, {int(manifest['large_bytes']) >> 20} MB in {manifest['large']} large files")

    whole = os.path.join(base, "whole")
    os.makedirs(whole)
    job = run_transfer(ctx, "local", "file://" + src, "file://" + whole, src, os.path.join(whole, "big"))

    # Undo of a large copy removes all of it, and nothing else.
    with open(os.path.join(whole, "mine.txt"), "w") as f:
        f.write("not the job's")
    d.call("Undo")
    gone = wait_for(lambda: not os.path.exists(os.path.join(whole, "big")) or None, timeout=120)
    c.check("local: undo removes everything the copy made", gone)
    c.check("local: and nothing it did not", os.listdir(whole) == ["mine.txt"], os.listdir(whole))

    part = os.path.join(base, "part")
    os.makedirs(part)
    _, got = cancel_midway(ctx, "local", "file://" + src, "file://" + part, part)
    before = walk(part)
    d.call("Undo")
    gone = wait_for(lambda: not os.path.exists(os.path.join(part, "big")) or None, timeout=120)
    c.check("local: what a cancelled copy did manage can be undone too", gone, sorted(before)[:3])
