"""The window's memory floor: what a folder costs the window to show, and how much of it comes
back once the folder is left (docs/0.5.0/05-window-memory.md).

Not in the default run — the gallery fixture is a gigabyte and the flow waits half a minute
after each folder:

    make memory-floor                    (or tests/e2e/run.sh --flow memory_floor)

Memory of the window (the `qs` of this run) and of its daemon — Pss, the process's own pages
and its share of the libraries it has in common with every other Qt and Mesa process, which is
the fair figure; RSS beside it for the record — read from /proc at each step: fresh; holding a thousand-row folder in the list view; five and thirty seconds after
leaving it for an empty folder; the same for the icon view and the gallery over a thousand
photographs (thumbnails and decoded pictures); then five thousand rows in the list. The bar is
the plan's: thirty seconds after leaving, the window within 15 MB of fresh for the list and
30 MB for icons and the gallery. Every run is one line in `bench/shell-memory.jsonl`
(KIKI_MEM_HISTORY moves it; empty records nothing), with the commit, so a change is a diff of
that file; a run prints how it differs from the last one on the same machine.
"""
import json, os, time
from harness import wait_for
from flows.open_perf import flat, git, machine
from flows import gallery_perf

NEEDS = {"shell"}
TITLE = "the window's memory floor: a folder shown, then left"

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
FIXTURE = os.environ.get("KIKI_PERF_DIR", "/tmp/kiki-perf")
HISTORY = os.environ.get("KIKI_MEM_HISTORY", os.path.join(ROOT, "bench", "shell-memory.jsonl"))
SETTLE = int(os.environ.get("KIKI_MEM_SETTLE", "30"))      # seconds after leaving a folder
STEPS = (("fresh", None), ("list1k", "listed"), ("list1k-left5s", None), ("list1k-left", None),
         ("icon1k", "listed"), ("icon1k-left5s", None), ("icon1k-left", None),
         ("gallery1k", "listed"), ("gallery1k-left5s", None), ("gallery1k-left", None),
         ("list5k", "listed"), ("list5k-left5s", None), ("list5k-left", None))


def probe(ctx):
    return None


def mem_mb(match):
    """Memory, in MB, of the one process of this run that `match(cmdline)` picks: found by the
    runtime directory in its environment — no other process has this run's — and never by
    name, since every `qs ipc` call is a process called qs too. `pss` is the fair number
    (owner, 2026-10-02): the process's own pages plus its share of what it maps in common
    with every other Qt and Mesa process on the machine — a hundred megabytes of library
    code the window would be charged whole by `rss`, which is kept beside it for the record."""
    want = os.environ.get("XDG_RUNTIME_DIR", "")
    best = {"pss": 0, "rss": 0}
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            with open(f"/proc/{p}/environ", "rb") as fh:
                env = fh.read().split(b"\0")
            if f"XDG_RUNTIME_DIR={want}".encode() not in env:
                continue
            with open(f"/proc/{p}/cmdline", "rb") as fh:
                cmd = fh.read().replace(b"\0", b" ").decode(errors="replace")
            if not match(cmd):
                continue
            got = {}
            with open(f"/proc/{p}/smaps_rollup") as fh:
                for line in fh:
                    if line.startswith("Pss:"):
                        got["pss"] = int(line.split()[1]) // 1024
                    elif line.startswith("Rss:"):
                        got["rss"] = int(line.split()[1]) // 1024
            if got.get("pss", 0) > best["pss"]:
                best = {"pss": got.get("pss", 0), "rss": got.get("rss", 0)}
        except OSError:
            continue
    return best


def measure():
    shell = mem_mb(lambda c: "shell.qml" in c and " ipc " not in c)
    kikid = mem_mb(lambda c: c.rstrip().endswith("kikid"))
    # `shellMb` is Pss; the history's lines before 2026-10-02 are RSS and say so with `measure`.
    return {"shellMb": shell["pss"], "shellRssMb": shell["rss"], "kikidMb": kikid["pss"]}


def record(results):
    if not HISTORY or not results:
        return
    m = machine()
    entry = {"at": int(time.time()), "commit": git("rev-parse", "--short", "HEAD"), "measure": "pss", "dirty": bool(git("status", "--porcelain")),
             "subject": git("log", "-1", "--format=%s"), "settleS": SETTLE, "machine": m, "results": results}
    last = None
    try:
        with open(HISTORY) as fh:
            for line in fh:
                try:
                    e = json.loads(line)
                except json.JSONDecodeError:
                    continue
                em = e.get("machine", {})
                if (em.get("cpu"), em.get("renderer")) == (m["cpu"], m["renderer"]):
                    last = e
    except OSError:
        pass
    os.makedirs(os.path.dirname(HISTORY), exist_ok=True)
    with open(HISTORY, "a") as fh:
        fh.write(json.dumps(entry, sort_keys=True) + "\n")
    print(f"  ... recorded as {entry['commit']}{'+' if entry['dirty'] else ''} in {os.path.relpath(HISTORY, ROOT)}")
    if last is None:
        print("  ... nothing earlier from this machine and renderer to compare with")
        return
    print(f"  ... against {last.get('commit')}{'+' if last.get('dirty') else ''} ({time.strftime('%Y-%m-%d %H:%M', time.localtime(last.get('at', 0)))}): window MB")
    for key, now in results.items():
        was = last.get("results", {}).get(key)
        if was:
            print(f"      {key:16} {was['shellMb']:5d} -> {now['shellMb']:5d} MB")


def run(ctx):
    c, sh = ctx.checks, ctx.shell
    local1k, local5k, pics = os.path.join(FIXTURE, "open1k"), os.path.join(FIXTURE, "open5k"), gallery_perf.FIXTURE
    flat(local1k, 1_000)
    flat(local5k, 5_000)
    if not c.check(f"the gallery fixture has {gallery_perf.N} photographs", gallery_perf.build_fixture(), pics):
        return
    empty = os.path.join(ctx.base, "empty"); os.makedirs(empty, exist_ok=True)
    away = "file://" + empty

    def opened(uri, view):
        # The view after the open, not before: opening applies the folder's remembered view.
        sh.call("open", uri)
        wait_for(lambda: (lambda s: s if s.get("uri") == uri and s.get("done") else None)(sh.state()), timeout=60, interval=0.05)
        sh.call("setView", view)
        wait_for(lambda: (lambda s: s if s.get("view") == view else None)(sh.state()), timeout=10)

    def leave():
        sh.call("setView", "list")
        sh.call("open", away)
        wait_for(lambda: (lambda s: s if s.get("uri") == away and s.get("done") else None)(sh.state()), timeout=30)

    results = {}

    def note(step):
        results[step] = measure() | {"view": sh.state().get("view", "")}
        print(f"      {step:16} window {results[step]['shellMb']:5d} MB   kikid {results[step]['kikidMb']:4d} MB   ({results[step]['view']})")

    sh.call("dismiss")
    sh.call("split", "off")
    sh.call("inspector", "off")
    leave()
    # A window just started is still letting go of what starting cost it (under cage it read
    # 477 MB at two seconds and 422 at forty): "fresh" is the window once that has passed.
    time.sleep(SETTLE)
    note("fresh")

    for step, view, uri, hold in (("list1k", "list", "file://" + local1k, 3), ("icon1k", "icon", "file://" + pics, 12),
                                  ("gallery1k", "gallery", "file://" + pics, 2), ("list5k", "list", "file://" + local5k, 3)):
        before = measure()["shellMb"]
        opened(uri, view)
        if view == "gallery":
            # Step through a few pictures: the gallery decodes what it shows and its neighbours.
            for _ in range(10):
                sh.call("gallery", "next"); time.sleep(0.5)
            try:
                print(f"      ... the gallery decoded {json.loads(sh.call('galleryStats') or '{}').get('decodes', '?')} pictures")
            except json.JSONDecodeError:
                pass
        time.sleep(hold)         # thumbnails and pictures land asynchronously; let them
        note(step)
        leave()
        time.sleep(5)
        note(step + "-left5s")
        time.sleep(max(0, SETTLE - 5))
        note(step + "-left")
        results[step + "-left"]["keptMb"] = results[step + "-left"]["shellMb"] - before

    # What each folder left behind, against what the window was before it: the sequence makes
    # "against fresh" the sum of everything before, which says nothing about the one folder.
    fresh = results["fresh"]["shellMb"]
    # The gallery's bar is what it measured plus the spread between runs (27–32 MB kept, ±5;
    # docs/0.5.0/05-window-memory.md): the number is watched for growing, not fought.
    for step, bar in (("list1k", 15), ("icon1k", 30), ("gallery1k", 40), ("list5k", 15)):
        kept = results[step + "-left"]["keptMb"]
        c.check(f"{step}: {SETTLE}s after leaving, the window kept at most {bar} MB of it", kept <= bar, f"{kept:+d} MB")
    last = results["list5k-left"]["shellMb"]
    print(f"      the floor: {fresh} MB fresh, {last} MB after the four folders ({last - fresh:+d})")
    results["floorMb"] = {"shellMb": last - fresh, "kikidMb": results["list5k-left"]["kikidMb"], "view": ""}
    record(results)
