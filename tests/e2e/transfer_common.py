"""What every large-transfer flow asserts, whichever way the bytes go (plan 31, phase 4).

The filesystem is the oracle: a tree is compared entry for entry, the large files hash for hash,
the small ones by sample. Beside that: progress that never lies (bytes only rise, totals are never
revised down, it ends where it said it would), a cancel that leaves nothing half-made, a daemon
that does not hold the tree in memory, and a number for how fast it went.

The fixture is `kikid bench gen <profile>`: `transfer-lite` by default (a minute), `transfer` for a
release (`KIKI_TRANSFER_PROFILE=transfer`: 50,000 files, 2.5 GB).
"""
import hashlib, json, os, random, subprocess, time
from harness import wait_for

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
PROFILE = os.environ.get("KIKI_TRANSFER_PROFILE", "transfer-lite")
TIMEOUT = int(os.environ.get("KIKI_TRANSFER_TIMEOUT", 1800 if PROFILE == "transfer" else 300))


def kikid():
    return os.environ.get("KIKID") or os.path.join(ROOT, "target", "release", "kikid")


def generate(into, profile=None):
    subprocess.run([kikid(), "bench", "gen", profile or PROFILE, into], check=True, capture_output=True)
    return dict(l.split() for l in open(os.path.join(into, "MANIFEST.txt")).read().splitlines())


def walk(root):
    """rel -> ('d' | 'l:<target>' | size) for everything under root, names as bytes-safe strings."""
    out = {}
    for dirpath, dirnames, filenames in os.walk(root):
        for n in dirnames + filenames:
            full = os.path.join(dirpath, n)
            rel = os.path.relpath(full, root)
            if os.path.islink(full):
                out[rel] = "l:" + os.readlink(full)
            elif os.path.isdir(full):
                out[rel] = "d"
            else:
                out[rel] = os.path.getsize(full)
    return out


def digest(path):
    h = hashlib.blake2b(digest_size=16)
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def same_content(src, dst, tree, sample=400):
    """Every large file, and a sample of the rest, hash for hash. Returns the ones that differ."""
    files = [r for r, v in tree.items() if isinstance(v, int)]
    big = [r for r in files if tree[r] >= (8 << 20)]
    rest = [r for r in files if tree[r] < (8 << 20)]
    random.Random(7).shuffle(rest)
    return [r for r in big + rest[:sample] if digest(os.path.join(src, r)) != digest(os.path.join(dst, r))]


class Watch:
    """A job's events, as they came: for saying whether its progress was honest."""

    def __init__(self, daemon, job):
        self.d, self.job = daemon, job

    def events(self):
        self.d.drain(0.05)
        return [e["job"] for e in self.d.events if e.get("event") == "JobEvent" and e["job"]["id"] == self.job]

    def wait_end(self, timeout=TIMEOUT):
        e = self.d.wait_event(lambda e: e.get("event") == "JobEvent" and e["job"]["id"] == self.job and e["job"]["state"] in ("done", "failed", "cancelled"), timeout=timeout)
        return e["job"] if e else None

    def honest(self):
        """(ok, why) — bytes and items never fall; totals, once said, never shrink."""
        ev = [j for j in self.events() if j["state"] == "running"]
        for a, b in zip(ev, ev[1:]):
            if b["bytes"] < a["bytes"] or b["done"] < a["done"]:
                return False, f"went backwards: {a['done']}/{a['bytes']} then {b['done']}/{b['bytes']}"
            if a["total"] and b["total"] < a["total"] or a["bytesTotal"] and b["bytesTotal"] < a["bytesTotal"]:
                return False, f"a total shrank: {a['total']}/{a['bytesTotal']} then {b['total']}/{b['bytesTotal']}"
        return True, f"{len(ev)} progress events"


def rss_mb(pid):
    try:
        for line in open(f"/proc/{pid}/status"):
            if line.startswith("VmRSS:"):
                return int(line.split()[1]) / 1024
    except OSError:
        pass
    return 0.0


def daemon_pid():
    out = subprocess.run(["pgrep", "-n", "-f", kikid() + "$"], capture_output=True, text=True).stdout.split()
    return int(out[0]) if out else 0


def record(name, **numbers):
    """Beside plan 26's baselines, in the same shape: a regression shows up as a number."""
    path = os.path.join(os.environ.get("KIKI_E2E_OUT", "/tmp"), "transfers.json")
    try:
        all_ = json.load(open(path))
    except (OSError, ValueError):
        all_ = {}
    all_[f"{name}:{PROFILE}"] = numbers
    with open(path, "w") as f:
        json.dump(all_, f, indent=2, sort_keys=True)
    print("  " + name + ": " + ", ".join(f"{k} {v}" for k, v in numbers.items()))


def run_transfer(ctx, tag, src_uri, dst_uri, src_disk, dst_disk, expect_missing=(), tolerate=()):
    """Copy `src` into `dst`, watched; assert it arrived whole and honestly; record its numbers.

    `expect_missing`: names the destination cannot hold at all (a newline over FTP), which must
    fail BY NAME while everything else arrives. `tolerate`: things a destination legitimately
    alters (a symlink a server cannot make)."""
    c, d = ctx.checks, ctx.daemon
    tree = walk(src_disk)
    wanted = {r: v for r, v in tree.items() if not any(r == m or r.startswith(m + "/") for m in expect_missing)}
    pid = daemon_pid()
    peak = [rss_mb(pid)]
    idle = peak[0]
    d.events.clear()
    t0 = time.time()
    job = d.ok("Submit", op={"op": "copy", "items": [src_uri], "dest": dst_uri})["job"]
    w = Watch(d, job)

    # While it runs: the daemon still answers a listing it has never seen, inside its budget.
    probe = ctx.fixture({"probe": {f"p{i}.txt": "" for i in range(200)}})
    over = lambda: any(j["state"] in ("done", "failed", "cancelled") for j in w.events())
    wait_for(lambda: any(j["state"] == "running" and j["bytes"] > 0 for j in w.events()) or over() or None, timeout=TIMEOUT)
    t1 = time.time()
    lid = ctx.lid()
    d.ok("Open", lid=lid, uri="file://" + probe + "/probe")
    rows = wait_for(lambda: d.ok("Window", lid=lid, first=0, count=60)["rows"] or None, timeout=5)
    listed_ms = (time.time() - t1) * 1000
    d.call("Close", lid=lid)
    still = [j for j in w.events()][-1]["state"] == "running"
    if still:
        c.check(f"{tag}: with the transfer running, another folder still lists at once", rows is not None and listed_ms < 500, f"{listed_ms:.0f} ms")
    else:
        # Said, not passed: a check that could not have failed proves nothing. (A local copy on a
        # filesystem that clones is over in a blink; use KIKI_TRANSFER_PROFILE=transfer.)
        print(f"  NOTE {tag}: over before anything could be asked of it mid-run — responsiveness not observed at this size")

    ended = None
    while ended is None:
        peak.append(rss_mb(pid))
        ended = wait_for(lambda: next((j for j in w.events() if j["state"] in ("done", "failed", "cancelled")), None), timeout=1)
        if time.time() - t0 > TIMEOUT:
            # Never walk away from a job that is still running: what comes next would be piled
            # on top of it, and its failures blamed on something else.
            # What was it doing? This is what the job's log is for.
            log = d.ok("JobLog", job=job)
            print(f"  {tag}: stalled — the last lines of its log ({log['next']} in all):")
            for l in log["lines"][-14:]:
                print(f"    +{(l['t'] - log['lines'][0]['t']) / 1000:7.1f}s {l['level']:5} {l['source'][:26]:26} {l['text'][:150]}")
            d.ok("Cancel", job=job)
            w.wait_end(timeout=120)
            c.check(f"{tag}: finished inside {TIMEOUT} s", False, "cancelled by the test")
            break
    secs = time.time() - t0
    if expect_missing:
        err = (ended or {}).get("error") or ""
        n = len(expect_missing)
        c.check(f"{tag}: what this destination cannot hold fails BY NAME and BY COUNT — {n} of them, and only those", ended and ended["state"] == "failed" and err.startswith(f"{n} of ") and "awkward/" in err, err)
    else:
        c.check(f"{tag}: the job ends done", ended and ended["state"] == "done", ended and (ended["state"], ended.get("error")))
    if secs > 1.5:
        c.check(f"{tag}: it said how fast it was going, and which file it was on", any(j.get("rate", 0) > 0 for j in w.events()) and any((j.get("current") or {}).get("name") for j in w.events()))
    ok, why = w.honest()
    c.check(f"{tag}: progress was honest — nothing went backwards, no total shrank", ok, why)
    if ended and ended["state"] == "done":
        c.check(f"{tag}: and it ended where it said it would", ended["done"] == ended["total"] and ended["bytes"] == ended["bytesTotal"], (ended["done"], ended["total"], ended["bytes"], ended["bytesTotal"]))

    got = walk(dst_disk)
    missing = sorted(r for r in wanted if r not in got and r not in tolerate)
    extra = sorted(r for r in got if r not in tree and not r.endswith(".kiki-part"))
    wrong = sorted(r for r in wanted if r in got and got[r] != wanted[r] and r not in tolerate)
    c.check(f"{tag}: every entry arrived — {len(wanted)} of them", not missing, missing[:5])
    c.check(f"{tag}: nothing that was not sent, and no part files left", not extra and not any(r.endswith(".kiki-part") for r in got), extra[:5])
    c.check(f"{tag}: every size is right, every folder a folder, every link a link", not wrong, [(r, wanted[r], got[r]) for r in wrong[:5]])
    differ = same_content(src_disk, dst_disk, {r: v for r, v in wanted.items() if r in got and r not in tolerate})
    c.check(f"{tag}: the large files and a sample of the small ones match, hash for hash", not differ, differ[:5])

    total = sum(v for v in wanted.values() if isinstance(v, int))
    grew = max(peak) - idle
    c.check(f"{tag}: the daemon did not hold the tree in memory (it grew {grew:.0f} MB moving {total / (1 << 20):.0f} MB)", grew < 150, f"{idle:.0f} -> {max(peak):.0f} MB")
    record(tag, seconds=round(secs, 1), mb_s=round(total / (1 << 20) / max(secs, 0.001), 1), files=len([v for v in wanted.values() if isinstance(v, int)]), peak_rss_mb=round(max(peak)), rss_growth_mb=round(grew))
    return job


def cancel_midway(ctx, tag, src_uri, dst_uri, dst_disk):
    """Cancel once bytes are moving: it ends cancelled, and nothing under its final name is half a file."""
    c, d = ctx.checks, ctx.daemon
    d.events.clear()
    job = d.ok("Submit", op={"op": "copy", "items": [src_uri], "dest": dst_uri})["job"]
    w = Watch(d, job)
    over = lambda: any(j["state"] in ("done", "failed", "cancelled") for j in w.events())
    wait_for(lambda: any(j["state"] == "running" and j["done"] >= 20 for j in w.events()) or over() or None, timeout=TIMEOUT)
    t0 = time.time()
    d.ok("Cancel", job=job)
    ended = w.wait_end(timeout=60)
    if ended and ended["state"] == "done" and not any(j.get("cancelling") and j["state"] == "running" for j in w.events()):
        print(f"  NOTE {tag}: finished before it could be cancelled — cancel not observed at this size")
    else:
        c.check(f"{tag}: cancelled mid-run, it ends as cancelled — and promptly", ended and ended["state"] == "cancelled" and time.time() - t0 < 10, (ended and ended["state"], f"{time.time() - t0:.1f}s"))
    got = walk(dst_disk)
    c.check(f"{tag}: and no part file is left behind", not any(r.endswith(".kiki-part") for r in got), [r for r in got if r.endswith(".kiki-part")][:3])
    return job, got
