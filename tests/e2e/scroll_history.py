#!/usr/bin/env python3
"""The scroll test's record, one line a run: tests/e2e/scroll_history.py [history.jsonl] [--all]

Blank frames (a row in view that was not there yet) and the p95 frame for each view and pace,
oldest first. Runs from other machines or renderers are left out unless --all: a software-rendered
run and one on a GPU are not the same measurement.
"""
import json
import os
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
args = [a for a in sys.argv[1:] if not a.startswith("--")]
path = args[0] if args else os.environ.get("KIKI_SCROLL_HISTORY") or os.path.join(ROOT, "bench", "scroll-history.jsonl")
try:
    runs = [json.loads(l) for l in open(path) if l.strip()]
except OSError:
    sys.exit(f"no history at {path}: run tests/e2e/run.sh --flow scroll_perf")
if not runs:
    sys.exit("the history is empty")
if "--all" not in sys.argv:
    m = runs[-1]["machine"]
    runs = [r for r in runs if (r["machine"].get("cpu"), r["machine"].get("renderer")) == (m.get("cpu"), m.get("renderer"))]
    print(f"{m.get('cpu')} · {m.get('renderer')} renderer · {len(runs)} runs (--all for every machine)\n")
keys = [k for k in ("list/slow", "list/fast", "icon/slow", "icon/fast", "columns/slow", "columns/fast") if any(k in r["results"] for r in runs)]
print(f"{'when':16} {'build':10} " + " ".join(f"{k:>14}" for k in keys) + "   commit")
for r in runs:
    cells = []
    for k in keys:
        s = r["results"].get(k)
        cells.append(f"{100 * s['blankFrames'] / max(1, s['frames']):5.1f}% {s['p95Ms']:4d}ms" if s else " " * 13)
    build = (r.get("commit") or "?") + ("+" if r.get("dirty") else "")
    print(f"{time.strftime('%Y-%m-%d %H:%M', time.localtime(r['at']))} {build:10} " + " ".join(f"{c:>14}" for c in cells) + f"   {r.get('subject', '')[:50]}")
