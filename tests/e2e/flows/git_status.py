"""Git badges, and their staying true when git is used from a terminal (plan 15).

A real repository with one file of each state. The listing's own watch sees FILES change; what
it cannot see is `git add`, a commit or a checkout — those touch nothing that is listed and change
every badge. The daemon watches `.git` for that, and this flow is what proves it: the rows change,
and the windows are told `RepoChanged`, without anything being listed again.

The second half is a folder of projects — `~/Projects`, which is in no repository at all — where
each row is a repository of its own and wears a branch capsule for it.
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


def capsules(d, lid):
    """What each row says for itself as a repository: branch and state, or None for a row that
    is not one (plan 15's capsule)."""
    rows = d.ok("Window", lid=lid, first=0, count=60)["rows"]
    out = {}
    for r in rows:
        g = r.get("git") or {}
        out[r["name"]] = (g.get("branch"), g.get("state")) if g.get("root") else None
    return out


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

    # The same thing on screen. Everything above reads the daemon's rows; this is the window's
    # half of it — a letter really drawn in a listing, and really gone again after a commit made
    # in a terminal. Guarded rather than declared in NEEDS, so the daemon-only run keeps the
    # checks above instead of skipping the whole flow for want of a window.
    sh = ctx.shell
    drawn = bool(sh.state())
    if drawn:
        sh.open("file://" + repo)
        sh.call("setView", "list")
        c.check("a badge is drawn in the window", sh.wait_geometry("git-badge") is not None, sh.state())

    # A commit clears what it committed.
    git(repo, "commit", "-qam", "second")
    clean = wait_for(lambda: (lambda s: s if s.get("tracked.txt") == "clean" and s.get("new.txt") == "clean" and s.get("src") == "clean" else None)(states(d, lid)), timeout=3)
    c.check("`git commit` clears the badges it committed", clean is not None, states(d, lid))
    if drawn:
        c.check("and the window stops drawing them, without listing the folder again",
                wait_for(lambda: (sh.geometry("git-badge") is None) or None, timeout=8) is not None,
                sh.geometry("git-badge"))

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

    capsule_rows(ctx)


def capsule_rows(ctx):
    """A folder of projects (plan 15). `~/Projects` is in no repository, so nothing above the
    rows knows anything about them: each project's row has to name its own branch and be
    coloured by its own state. The branch is a file read and comes with the listing; the state
    is a `git` per project and follows."""
    c, d = ctx.checks, ctx.daemon
    root = ctx.fixture({"alpha": {"a.txt": "a\n"}, "beta": {"b.txt": "b\n"}, "gamma": {"g.txt": "g\n"}, "notes": {"plain.txt": "p\n"}, "loose.txt": "x\n"})
    for name, branch in (("alpha", "main"), ("beta", "develop"), ("gamma", "main")):
        p = os.path.join(root, name)
        git(p, "init", "-q", "-b", branch)
        git(p, "add", "-A")
        git(p, "commit", "-qm", "first")
    # One of them is left on a detached HEAD, for the picture at the end.
    head = subprocess.run(["git", "-C", os.path.join(root, "gamma"), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    git(os.path.join(root, "gamma"), "checkout", "-q", head)

    lid = ctx.lid()
    d.ok("Open", lid=lid, uri="file://" + root)
    want = {"alpha": ("main", "clean"), "beta": ("develop", "clean"), "notes": None, "loose.txt": None}
    got = wait_for(lambda: (lambda s: s if all(s.get(k) == v for k, v in want.items()) else None)(capsules(d, lid)), timeout=8)
    c.check("a folder of projects names each one's branch, and leaves everything else alone", got is not None, capsules(d, lid))
    c.check("a detached project shows its short hash", (capsules(d, lid).get("gamma") or (None,))[0] == head[:8], capsules(d, lid).get("gamma"))

    # Dirty one of them. Nothing the folder is watching changes — inotify does not descend — so
    # this is what a refresh, or the next listing of the folder, is for.
    with open(os.path.join(root, "alpha", "new.txt"), "w") as f:
        f.write("n\n")
    d.ok("GitRefresh", uri="file://" + root)
    dirtied = wait_for(lambda: (lambda s: s if s.get("alpha") == ("main", "untracked") else None)(capsules(d, lid)), timeout=8)
    c.check("dirtying one project recolours that project", dirtied is not None, capsules(d, lid))
    c.check("…and only that one", capsules(d, lid).get("beta") == ("develop", "clean"), capsules(d, lid))

    # `git add` moves the index, and the daemon stats it rather than spending a watch on it:
    # forty projects in a folder would want forty, against sixty-four for the whole daemon.
    d.events.clear()
    git(os.path.join(root, "alpha"), "add", "new.txt")
    added = wait_for(lambda: (lambda s: s if s.get("alpha") == ("main", "added") else None)(capsules(d, lid)), timeout=8)
    c.check("`git add` inside a project reaches its capsule without a watch of its own", added is not None, capsules(d, lid))
    c.check("and no listing was reset to do it", not any(e.get("event") == "Reset" and e.get("lid") == lid for e in d.events))

    # A commit from outside clears it, and a new branch renames it — both from a terminal.
    git(os.path.join(root, "alpha"), "commit", "-qm", "second")
    cleared = wait_for(lambda: (lambda s: s if s.get("alpha") == ("main", "clean") else None)(capsules(d, lid)), timeout=8)
    c.check("a commit made outside kiki clears the project's colour", cleared is not None, capsules(d, lid))
    git(os.path.join(root, "beta"), "checkout", "-qb", "release/2")
    renamed = wait_for(lambda: (lambda s: s if s.get("beta") == ("release/2", "clean") else None)(capsules(d, lid)), timeout=8)
    c.check("`git checkout -b` changes what the capsule says", renamed is not None, capsules(d, lid))

    # A picture of the three of them together: clean, dirty and detached.
    sh = ctx.shell
    if sh.state():
        with open(os.path.join(root, "beta", "b.txt"), "a") as f:
            f.write("edited\n")
        d.ok("GitRefresh", uri="file://" + root)
        wait_for(lambda: (lambda s: s if s.get("beta") == ("release/2", "modified") else None)(capsules(d, lid)), timeout=8)
        sh.open("file://" + root)
        sh.call("setView", "list")
        drawn = sh.wait_geometry("git-capsule")
        c.check("the capsule is drawn in the window", drawn is not None, sh.state())
        if shutil.which("grim"):
            subprocess.run(["grim", os.path.join(os.environ.get("KIKI_E2E_OUT", "/tmp"), "git-capsule.png")], capture_output=True)
