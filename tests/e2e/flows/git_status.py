"""Git badges, and their staying true when git is used from a terminal (plan 15).

A real repository with one file of each state. The listing's own watch sees FILES change; what
it cannot see is `git add`, a commit or a checkout — those touch nothing that is listed and change
every badge. The daemon watches `.git` for that, and this flow is what proves it: the rows change,
and the windows are told `RepoChanged`, without anything being listed again.
"""
import os, shutil, subprocess
from harness import wait_for

NEEDS = {"daemon"}
TITLE = "git status in a listing, and after git is used outside kiki"


def git(root, *args):
    env = dict(os.environ, GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_SYSTEM="/dev/null",
               GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.invalid", GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@example.invalid")
    subprocess.run(["git", "-C", root, "-c", "init.defaultBranch=main", *args], check=True, capture_output=True, env=env)


def states(d, lid):
    rows = d.ok("Window", lid=lid, first=0, count=60)["rows"]
    return {r["name"]: (r.get("git") or {}).get("state", "clean") for r in rows}


def run(ctx):
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"repo": {"tracked.txt": "one\n", "staged.txt": "s\n", "gone.txt": "g\n", "build": {"out.o": "x"}, "src": {"main.rs": "fn main() {}\n"}, ".gitignore": "build/\n*.log\n"}})
    repo = os.path.join(root, "repo")
    git(repo, "init", "-q")
    git(repo, "add", "-A")
    git(repo, "commit", "-qm", "first")
    with open(os.path.join(repo, "tracked.txt"), "a") as f:
        f.write("two\n")
    with open(os.path.join(repo, "src", "main.rs"), "a") as f:
        f.write("// edited\n")
    with open(os.path.join(repo, "new.txt"), "w") as f:
        f.write("n\n")
    with open(os.path.join(repo, "debug.log"), "w") as f:
        f.write("l\n")
    os.remove(os.path.join(repo, "gone.txt"))

    lid = ctx.lid()
    d.ok("Open", lid=lid, uri="file://" + repo)
    want = {"tracked.txt": "modified", "new.txt": "untracked", "debug.log": "ignored", "build": "ignored", "src": "modified", "staged.txt": "clean"}
    got = wait_for(lambda: (lambda s: s if all(s.get(k) == v for k, v in want.items()) else None)(states(d, lid)), timeout=5)
    c.check("each row carries its state; a folder carries its subtree's", got is not None, states(d, lid))

    # git add, from outside: nothing listed changes on disk, only .git/index.
    d.drain(0.2)
    d.events.clear()
    git(repo, "add", "new.txt")
    added = wait_for(lambda: states(d, lid).get("new.txt") == "added" or None, timeout=3)
    c.check("`git add` in a terminal turns untracked into added, without a re-list", added, states(d, lid).get("new.txt"))
    c.check("and the window is told the repository changed", any(e.get("event") == "RepoChanged" for e in d.events) or d.wait_event(lambda e: e.get("event") == "RepoChanged", timeout=2) is not None)
    c.check("no listing was reset to do it", not any(e.get("event") == "Reset" and e.get("lid") == lid for e in d.events))

    # A commit clears what it committed.
    git(repo, "commit", "-qam", "second")
    clean = wait_for(lambda: (lambda s: s if s.get("tracked.txt") == "clean" and s.get("new.txt") == "clean" and s.get("src") == "clean" else None)(states(d, lid)), timeout=3)
    c.check("`git commit` clears the badges it committed", clean is not None, states(d, lid))

    # A new branch is news for the chip.
    d.events.clear()
    git(repo, "checkout", "-qb", "feature")
    told = d.wait_event(lambda e: e.get("event") == "RepoChanged", timeout=3)
    r = d.ok("Repo", uri="file://" + repo)
    c.check("`git checkout -b` reaches the branch chip", told is not None and r and r.get("branch") == "feature", r)

    # [git] showIgnored = "hide": ignored rows leave the listing, as dot-files do.
    d.ok("SetSettings", patch={"git": {"showIgnored": "hide"}})
    sub = os.path.join(root, "repo2")
    shutil.copytree(repo, sub)
    lid2 = ctx.lid()
    d.ok("Open", lid=lid2, uri="file://" + sub)
    hidden = wait_for(lambda: (lambda s: s if s and "debug.log" not in s and "build" not in s and "tracked.txt" in s else None)(states(d, lid2)), timeout=5)
    c.check("with ignored files hidden, they are not in the listing at all", hidden is not None, sorted(states(d, lid2)))
    d.ok("SetSettings", patch={"git": {"showIgnored": "dim"}})
