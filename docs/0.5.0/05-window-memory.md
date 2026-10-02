# 05 — The window's memory floor, and the window's share of a first paint

**Status:** built, 2026-10-01 — measured first, then one thing fixed and three tried and left; the flow and its history kept.

The plan began as a memory question (`docs/0.1.0/27-gallery-view.md`, 2026-09-19: "the
window's floor rises with the largest folder visited: fresh shell 249 MB, +62 MB once a
1 000-row folder has been shown, and it does not come back") and grew a second half from what
plan 10 found: that a cold open's time is the window's, not the daemon's — a `done` the daemon
sent at 5 ms reached the window's probe at 117 ms under `cage`. Both are the window's handling
of a listing, and both were measured by the same probes.

## What was found

**The first paint.** The views' model is the listing's count (`model: root.pane.listing.count`),
and a model that is a number is a *new* model each time it changes: the view tears down every
delegate it has and builds them again. A scan sends a `Count` per chunk as the folder is read —
twelve for 10 000 files — and each one rebuilt the screen for rows that had not arrived.
Measured by `UI.OpenProbe` (now counting the `Count` events, the milliseconds inside
`handleEvent`, and the delegates built, `make open-perf`, median of five):

| opening | Count events | ms in events before | after | delegates built before | after | first rows before | after |
|---|---|---|---|---|---|---|---|
| 10 000 cold, cage | 12 | 108 | 20 | 215 | 21 | 118 ms | **31 ms** |
| 10 000 cold, desktop | 12 | — | 10 | — | 8 | 56 ms | **17 ms** |
| 1 000 cold, desktop | 3 | — | 9 | — | 8 | 19 ms | **13 ms** |
| cached, any | 1 | | 4–5 | | 8 | ~10 ms | ~10 ms |

The fix is in `WindowCache.qml`: a count still growing is applied at most four times a second
(`countThrottle`, 250 ms); the final count, and a scan that ends within the wait, at once. A
local scan ends inside the wait, so its screen is built once; a remote scan that takes seconds
shows its count growing four times a second instead of per chunk. Together with plan 10 a cold
open of 10 000 files on the desktop went 74 → 17 ms.

**The floor.** `tests/e2e/flows/memory_floor.py` (`make memory-floor`; `bench/shell-memory.jsonl`),
the window's memory read from `/proc` by the run's own runtime directory — **Pss**, the
process's own pages plus its share of the libraries it maps in common with every other Qt and
Mesa process, which is the fair figure (owner, 2026-10-02; RSS charges the window the whole of
a hundred megabytes of shared library code and is kept beside it only for the record): fresh
after thirty seconds (a window just started is still letting go of what starting cost it),
then a thousand rows in the list view, left for an empty folder, read again at five and thirty
seconds; the same for the icon view over a thousand photographs, the gallery stepping through
eleven of them, and five thousand rows. On the owner's desktop (the GPU renderer; `cage`'s
software renderer sits higher throughout and moves the same way):

| folder | window while shown | kept 30 s after leaving |
|---|---|---|
| 1 000 rows, list | +9 MB | +9 MB |
| 1 000 photographs, icons (thumbnails) | +4 MB | +1 MB |
| 1 000 photographs, gallery, 11 decoded | +29 MB | +22 MB |
| 5 000 rows, list | +1 MB | +1 MB |
| the four together | | **+33 MB** over 187 MB fresh |

(The RSS run of the day before read 267 MB fresh and +23 to +38 after the four; the 80 MB
between the two figures is shared library code — Qt, Mesa's `libgallium` and the `libLLVM` it
loads — charged in full by RSS and by its share in Pss. Of the 187, about 137 is the window's
own: the JS engine with kiki's QML compiled, the scene graph and its textures, glyph and icon
caches, the GL driver's buffers — the ordinary size of a Qt Quick window of this scope.)

The 62 MB a folder in the 09-19 note does not reproduce: a thousand rows cost 8 MB and five
thousand one more. What stays is the gallery's — eleven 1600×1200 pictures decoded, and
22–32 MB of it kept after the view is gone. Three suspects for that, each one run alone:

| suspect | gallery kept after | verdict |
|---|---|---|
| none (baseline) | +27 to +30 MB | |
| `gc()` when a listing is left | +30 MB | no change — not kept |
| `Image.cache: false` on the gallery's three thumbnail images | +29 MB | no change — not kept |
| glibc trimming for the window (`MALLOC_TRIM_THRESHOLD_`, `MALLOC_MMAP_THRESHOLD_`, `MALLOC_ARENA_MAX`) | +27 MB | no change — not kept |
| the Quick Look window behind a `Loader` (fresh, not kept-after) | 187 → 185 MB fresh | 2 MB — not kept. Tried on a wrong reading: a bare Quickshell window is 96 MB and one with QtMultimedia 133, so the player looked like 37 of kiki's 91; but `QuickLookWindow` already loads its faces by file name, and QtMultimedia is not mapped until the first video — the running window has no `libQt6Multimedia` at all. The 91 is the QML itself |

The spread between runs is ±5 MB, and none of the three moved the number by more than that.
What is left is below QML: the scene graph's textures and the GL driver's copies of them,
which the gallery's pane hands back when its `Loader` destroys it and the driver keeps. The
rows themselves (decision 2's third suspect) are released — a list folder keeps nothing a
second folder does not — and the views' `cacheBuffer` is a screen or two of delegates, not
tens of megabytes; neither was run as an experiment because the numbers gave them nothing to
explain. So, per decision 3: the floor is watched, not fought — the flow stays, its bars are
per folder (what each one *kept*, against the window before it opened: 15 MB for a list,
30 MB for a folder of thumbnails, 40 MB for the gallery — what it measured plus the spread),
and a run records to `bench/shell-memory.jsonl` beside the commit.

## Decisions, as planned (2026-09-29)

1. **Measure before touching** — `memory_floor.py`, above.
2. **The suspects, each measured alone** — above; the one that was not on the list (the count
   per chunk rebuilding the screen) was found by the churn counters added to `OpenProbe`.
3. **The bar** — per folder now, for the reason above; a laptop that browses a photo library
   all afternoon is 38 MB heavier for its first gallery session and not for its tenth folder.

## Verification

`make open-perf` on the desktop (medians of five, recorded in `bench/open-history.jsonl`) and
`make memory-floor` on the desktop (`bench/shell-memory.jsonl`), both after the full gate;
`tst_WindowCache` covers the throttle by what it already asserts (a `Count` with `done` applies
at once; the fake daemon sends no partial counts).
