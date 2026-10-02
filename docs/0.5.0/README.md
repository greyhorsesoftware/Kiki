# 0.5.0

The plans for the release after 0.4.x, in no settled order. All **planned 2026-09-29**; sizes
are in each.

1. `01-ui-cleanup.md` — **small things seen in daily use**, each with where it is and what done
   looks like: the rename box sitting right of the name it replaces; the status message
   shorter and without its ×; the drag's + shown whenever the drop will copy — across machines
   and across mountpoints. Added to as they are found.
2. `02-daemon-bounds.md` — **the daemon's last unbounded things** from the audit: the writer
   channel to a window, `Plugin::shutdown()` with no deadline, a ceiling on a plugin request and
   on `bsdtar`, the mirror's audit log.
3. `03-fuzz-json.md` — **the JSON reader fuzzed for real**: `cargo-fuzz` targets and a corpus
   in the tree, CI's warning-that-is-not-coverage replaced by a step that fails.
4. `04-shell-split.md` — **`Shell.qml` split along its seams** (1 866 lines): IPC, keys, menus,
   drag, chooser out, no test edited.
5. `05-window-memory.md` — **the window's memory floor** measured (a flow and a history file)
   and the window's share of a first paint: the count-per-chunk screen rebuild found and fixed
   (10 000 files cold 56 → 17 ms on the desktop); the floor measured at 38 MB after four
   folders, the gallery's, and left to the driver. **Built 2026-10-01.**
6. `06-index-live.md` — **the index keeps up**: watched folders feed it as they change, the
   refresh descends only where an mtime moved.
7. `07-measure-before-tag.md` — **the benchmarks run again** before the tag, on the baseline's
   machine, with the thumbnail stat per row fixed if it is still 4 ms.
8. `08-ci-once.md` — **CI builds once**: one run read with a stopwatch, then tests on the
   release profile, the release workflow warm from the CI cache, the e2e suite in two `cage`s.
9. `09-loose-ends.md` — the fail marker's day as a setting, the demo folders in the home, the
   0.2 video flow, a re-composite flag for the recorder.
10. `10-faster-listings.md` — **the first screenful comes with the open**: rows ride on the
    `Open` reply or the `Reset`, the window never asks a `Window` for them, every `Reset`
    carries the rows on screen. Local and remote. Measured first.
