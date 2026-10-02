"""Opening a folder: how long until its first rows are there, and how many requests it took.

Not in the default run — the fixtures are built once and the flow takes a minute:

    make open-perf                       (or tests/e2e/run.sh --flow open_perf)

The number docs/0.5.0/10-faster-listings.md exists to change is `requestsAtRows`: the `Window`
requests the window made before the first screenful was there to draw — one today, nought
when the rows ride on the open. Beside it, measured from the window's side of the socket by
`UI.OpenProbe`: `firstRowsMs` (the rows there to draw), `firstFrameMs` (the frame after that),
`firstMetaMs` (their sizes and dates landed), `doneMs` (the count final).

Six openings: a thousand files and ten thousand, each cold (the daemon has never listed it)
and cached (listed, left, come back to); and a thousand on a real `sshd`, cold and cached.
Every run is recorded as one line of JSON in `bench/open-history.jsonl` (KIKI_OPEN_HISTORY
moves it; empty records nothing), with the commit, so a change is a diff of that file. A run
prints how it differs from the last one on the same machine. Nothing is asserted but that the
openings happened: the point is the shape, before and after.
"""
import getpass, json, os, shutil, subprocess, time
from harness import wait_for
from servers import Servers, add_location, sshd_bin

NEEDS = {"shell"}
TITLE = "opening a folder, timed: first rows, first frame, requests"

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
FIXTURE = os.environ.get("KIKI_PERF_DIR", "/tmp/kiki-perf")
# Each opening this many times; what is recorded is the median. One opening on a desktop that
# is doing other things varies by half its own length (10 000 files cold read 52 ms and then
# 76 ms in consecutive runs, 2026-10-01); five medians read the same to a couple of ms.
REPEAT = int(os.environ.get("KIKI_PERF_REPEAT", "5"))
HISTORY = os.environ.get("KIKI_OPEN_HISTORY", os.path.join(ROOT, "bench", "open-history.jsonl"))
KEPT = ("firstRowsMs", "firstFrameMs", "firstMetaMs", "doneMs", "requestsAtRows", "requests", "countEvents", "eventMs", "delegates", "rows", "cached")
# Opened in this order, so "cached" is a folder the daemon listed a moment ago and was left.
CASES = ("local1k", "local10k", "sftp1k")


def probe(ctx):
    return None if sshd_bin() else "sshd is not installed (sudo pacman -S openssh)"


def flat(dir, n):
    """`n` small files of a few kinds, like `kikid bench gen flat10k` makes. Kept between runs."""
    if os.path.isdir(dir) and len(os.listdir(dir)) >= n:
        return
    shutil.rmtree(dir, ignore_errors=True)
    os.makedirs(dir)
    kinds = (".txt", ".jpg", ".pdf", ".rs", ".md", ".png", "")
    for i in range(n):
        with open(os.path.join(dir, f"file {i:06d}{kinds[i % len(kinds)]}"), "w") as f:
            f.write("x" * (i % 7))


def git(*args):
    try:
        return subprocess.run(["git", "-C", ROOT, *args], capture_output=True, text=True, timeout=10).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def machine():
    cpu = ""
    try:
        with open("/proc/cpuinfo") as fh:
            cpu = next((l.split(":", 1)[1].strip() for l in fh if l.startswith("model name")), "")
    except OSError:
        pass
    return {"cpu": cpu, "cores": os.cpu_count() or 0, "kernel": os.uname().release,
            "renderer": os.environ.get("WLR_RENDERER") or "desktop", "backend": os.environ.get("WLR_BACKENDS") or ""}


def record(results):
    """Append this run to the history and say how it differs from the last comparable one."""
    if not HISTORY or not results:
        return
    m = machine()
    entry = {"at": int(time.time()), "commit": git("rev-parse", "--short", "HEAD"), "repeat": REPEAT, "dirty": bool(git("status", "--porcelain")),
             "subject": git("log", "-1", "--format=%s"), "machine": m, "results": results}
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
    print(f"  ... against {last.get('commit')}{'+' if last.get('dirty') else ''} ({time.strftime('%Y-%m-%d %H:%M', time.localtime(last.get('at', 0)))}): first rows, requests before them")
    for key, now in results.items():
        was = last.get("results", {}).get(key)
        if not was:
            continue
        print(f"      {key:14} rows {was['firstRowsMs']:4d} -> {now['firstRowsMs']:4d} ms    requests {was['requestsAtRows']} -> {now['requestsAtRows']}")


def stats(sh):
    try:
        return json.loads(sh.call("openStats") or "{}")
    except json.JSONDecodeError:
        return {}


def opening(sh, uri):
    sh.call("openProbe", uri)
    r = wait_for(lambda: (lambda s: s if s and not s.get("running", True) else None)(stats(sh)), timeout=20, interval=0.05) or stats(sh)
    return {k: r.get(k) for k in KEPT} | {"timedOut": r.get("timedOut", False), "error": r.get("error", "")}


def median(xs):
    xs = sorted(x for x in xs if isinstance(x, (int, float)))
    return xs[len(xs) // 2] if xs else None


def fold(runs):
    """One result from several: the median of each number, the worst of each flag."""
    r = {k: median([x[k] for x in runs]) for k in KEPT if k != "cached"}
    r["cached"] = all(x["cached"] for x in runs)
    r["timedOut"] = any(x["timedOut"] for x in runs)
    r["error"] = next((x["error"] for x in runs if x["error"]), "")
    return r


def run(ctx):
    c, sh, d = ctx.checks, ctx.shell, ctx.daemon
    local1k, local10k = os.path.join(FIXTURE, "open1k"), os.path.join(FIXTURE, "open10k")
    flat(local1k, 1_000)
    flat(local10k, 10_000)
    # The server's folder is the thousand, served over loopback; its keys live in the run's base.
    servers = Servers(os.path.join(ctx.base, "servers"))
    port, key = servers.start_sftp()
    r = add_location(d, {"name": "open-perf", "plugin": "sftp", "remoteUri": "sftp://open-perf" + local1k, "localUri": "",
                         "config": {"host": "127.0.0.1", "port": str(port), "username": getpass.getuser(), "auth": "key", "identityFile": key}}, {})
    if not c.check("the SFTP server is up and the location added", "ok" in r and not r["ok"].get("verify"), r):
        servers.stop()
        return
    src = {"local1k": local1k, "local10k": local10k, "sftp1k": local1k}
    uris = {"local1k": "file://" + local1k, "local10k": "file://" + local10k, "sftp1k": "sftp://open-perf" + local1k}
    away = "file://" + ctx.base      # somewhere else to stand between a cold open and a cached one
    scratch = os.path.join(FIXTURE, "open-scratch"); shutil.rmtree(scratch, ignore_errors=True); os.makedirs(scratch)

    results = {}
    try:
        sh.call("dismiss")
        sh.call("split", "off")
        sh.call("setView", "list")
        sh.open(away)
        for case in CASES:
            colds, cacheds = [], []
            for i in range(REPEAT):
                # Cold is a folder the daemon has never listed: a copy of the fixture, for this
                # opening only. (The server sees the same disk, so its copy is at the same path.)
                fresh = os.path.join(scratch, f"{case}-{i}")
                shutil.copytree(src[case], fresh)
                uri = uris[case].replace(src[case], fresh)
                colds.append(opening(sh, uri))
                sh.open(away)
                time.sleep(0.3)
                cacheds.append(opening(sh, uri))
                sh.open(away)
            cold, cached = fold(colds), fold(cacheds)
            results[case + "-cold"], results[case + "-cached"] = cold, cached
            for tag, r in (("cold", cold), ("cached", cached)):
                c.check(f"{case} {tag}: opened, {r['rows']} rows, nothing wrong", r["rows"] and not r["error"] and not r["timedOut"], r)
                print(f"      {case:9} {tag:6}  median of {REPEAT}:  rows {r['firstRowsMs']:4} ms  frame {r['firstFrameMs']:4} ms  meta {r['firstMetaMs']:4} ms  done {r['doneMs']:4} ms  "
                      f"requests before rows {r['requestsAtRows']}, in all {r['requests']}" + ("  (daemon cache)" if r["cached"] else ""))
                print(f"      {'':9} {'':6}  {'':12}   Count events {r['countEvents']}, {r['eventMs']} ms in events, {r['delegates']} delegates built")
    finally:
        d.call("RemoveLocation", name="open-perf")
        servers.stop()
        shutil.rmtree(scratch, ignore_errors=True)
    record(results)
