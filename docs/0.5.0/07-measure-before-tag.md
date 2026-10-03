# 07 — Measured again before the tag

**Status:** built, 2026-10-02 — the run made, one regression found and fixed, a new baseline
recorded, the run on the release checklist. The benchmarks left CI on 2026-09-23
(`26-benchmarks.md`: a GitHub runner is not the baseline's machine) and are run by hand,
pinned, before a tag — and had not been since. Everything after 0.2 landed unmeasured.

## The run (2026-10-02, the owner's machine, `taskset -c 0-3`, fixture on tmpfs)

HEAD (`5844d55`, every other 0.5 plan in) against a same-day build of the baseline commit
`6a6fc71`, `kikid bench compare`, each flagged line read and re-paired:

| line | base → head | verdict |
|---|---|---|
| `flat200k.rescan_ms` | 107 → 135 | **real.** Bisected to `5a2f4ff` ("4b", the decorations by name): `rescan` built a second `HashSet` of every present name — 200 000 more allocations — only so the decorations could drop the names that had gone, when the `old` map it already keeps is left holding exactly those. Fixed: the decorations forget `old`'s remainder (`deco.forget_names`), and the meta is taken out of `old` whether or not the scan brought its own, so a server's folder takes the same path. Same-minute A/B: **139 / 137 / 143 → 118 / 120 / 118 ms**; after the fix base → head reads 107 → 111. |
| `flat200k.index_query_us` | ~700 → ~980 (+40 %, stable over three pairs) | **known, and the machine.** The base predates the 09-23 change that ranks every match instead of the first 40 000 (+26 % that day). The rest is the day: every micro number on this machine read about twice the 09-19/09-23 figure today — base's own `index_query_us` 315 → 700, `phase1_first_chunk_ms` 3.5 → 12 — so head's 980 is the 09-23 398 at today's clock. At the 1 ms budget's edge only on a day like this; no code change. |
| `photos.thumbs_ms` | 0.6 → 51 | **not real.** Cache warm against cold: 50 real decodes are 51 ms (the 09-19 baseline read 81); the warm run before the pair makes both warm, which is how the recorded numbers were taken. |
| `flat200k.phase1_first_chunk_ms`, `sort_mtime_ms` (against the checked-in JSON only) | 3.5 → 12, 10 → 15 | **the machine** (base reads the same today). |

The two lines the plan named: `photos.window_meta_ms` **0.48 ms** (was 4 ms on 09-23) — the
confirmed-set of decision 2 is **not needed** and was not built. `index_query_us` as above.

Two more things kept from the run: `kikid bench` has `index_patch_ms` (what a rescan hands
the index, timed apart), and `index::patch_dir` skips the re-list when the folder's mtime has
not moved — the refresh's own rule, a real saving for a rescan that changed no name.

**The shell half** (same day, after the fix):

| flow | against | read |
|---|---|---|
| `scroll_perf` (cage, `bench/scroll-history.jsonl`) | 2026-09-20 | blank frames in the fling list 42.6 → **1.6 %**, icons 24.2 → 2.0 %, columns 5.2 → 0.8 %; p95 frame 24–28 → **18–20 ms** in every view — plans 10 and 05 (the first screenful unasked, the count no longer rebuilding the screen per chunk) |
| `gallery_perf` (cage) | — | first picture up 99 ms after the view opened (36 decoding); 30 steps 161 ms median wall, 42 ms average decode; a jump to row 500 up in 129 ms; 11 checks pass |
| `open_perf` (desktop, `bench/open-history.jsonl`) | 2026-10-02 morning | 1 000 cold 13 → 15 ms, cached 9 → 14; **10 000 cold 17 → 35 ms**; SFTP cold 307 → 302. Requests before rows 0 throughout. The 10 000 line is the one to read again on a quiet day before the tag: nothing on the open path changed since the morning's 17, and the daemon's own micro numbers read twice their usual on this machine all day (above) — if it still reads 35 on a day when `phase1_first_chunk_ms` reads 3.5, it is real |
| `memory_floor` (desktop, `bench/shell-memory.jsonl`) | 2026-10-02 morning | the floor after the four folders 33 → 15 MB; the gallery's kept 22 → 18; bars pass |

## Decisions, as planned (2026-09-29)

1. **The run is part of the release checklist** — `29-release-readiness.md`, section G.
2. **The thumbnail stat per row goes if it is still 4 ms** — it is 0.5; not touched.
3. **The shell half gets its numbers the same day** — run.
4. **A new baseline is recorded after the run** — `bench/baseline-linux-x86_64.json` is this
   run's `head.json`, with the `machine` object. Note its fixture is on **tmpfs**
   (`/tmp/kiki-perf/all`); the 09-19 one was on btrfs. A listing benchmark measures the
   filesystem as much as the code, so a compare against this file means a tmpfs fixture;
   the base-vs-head compare above was like against like.

## How it went wrong on the way, for next time

`/tmp` was cleared mid-run: the fixture, the baseline build and the bisect script vanished,
the agent's shell (whose working directory was under `/tmp`) died, and two bisects skipped
every commit before anyone noticed the fixture was gone. Bench fixtures are made under
`/tmp/kiki-perf` — that is where throwaway gigabytes belong — and **removed when the run or
flow ends**, success or not (owner, 2026-10-02: "tests should cleanup after themselves"): a
run never cd's into `/tmp`, rebuilds what it needs each time (`gallery1k` is a minute), and
leaves no `kiki-*` folder behind it.
