# 07 — Measured again before the tag

**Status:** planned, 2026-09-29. The benchmarks left CI on 2026-09-23 (`26-benchmarks.md`: a
GitHub runner is not the baseline's machine) and are run by hand, pinned, before a tag — and
have not been since. Everything after 0.2 — the decoder unification, the thumbnail fail
markers, the drag ghost's warm grab, the tunnel, the router over `handlers/*.rs`, the
lifetime — landed unmeasured. Nothing suggests a regression; nothing says there is none.

## Decisions

1. **The run is part of the release checklist**, written into `29-release-readiness.md`'s
   list beside the version bump: `taskset -c 0-3 kikid bench run <tree> --json out.json` against
   a same-day build of the baseline commit, and `bench compare`. The two numbers the last run
   flagged and left — `photos.window_meta_ms` (a stat per visible row for the thumbnail cache,
   1.6 → 4 ms) and `index_query_us` — are the first two lines read.
2. **The thumbnail stat per row goes if it is still 4 ms.** The window's rows ask whether a
   thumbnail exists once per paint; the daemon knows what it has written this session
   (`thumbs.rs` writes them) and can answer from a set instead of a `stat` — a
   `HashSet<PathBuf>` of what it has confirmed, bounded to 50 000 and cleared on a cache sweep.
   An hour, if the number says so; not touched if it does not.
3. **The shell half gets one number**: `scroll_perf` (which exists) and `gallery_perf` are run
   the same day and their lines appended to `bench/scroll-history.jsonl`, so the release has a
   frame-time figure beside the daemon's.
4. **A new baseline is recorded after the run**, on the same machine, with the `machine` object
   — the 09-19 baseline is a different kernel now.

## Size

An hour to run, an hour to read, and decision 2's hour if it is needed. The output is a
paragraph in this file's status line with the numbers, the way `26-benchmarks.md` records its
runs.
