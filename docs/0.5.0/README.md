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
4. `04-shell-split.md` — **`Shell.qml` split along its seams**: IPC, keys, menus, drag and
   chooser out into five files, 1 871 → 1 194 lines, no test edited; a drop-focus defect the
   split uncovered fixed on the way. **Built 2026-10-02.**
5. `05-window-memory.md` — **the window's memory floor** measured (a flow and a history file)
   and the window's share of a first paint: the count-per-chunk screen rebuild found and fixed
   (10 000 files cold 56 → 17 ms on the desktop); the floor measured at 38 MB after four
   folders, the gallery's, and left to the driver. **Built 2026-10-01.**
6. `06-index-live.md` — **the index keeps up**: watched folders feed it as they change, the
   refresh descends only where an mtime moved.
7. `07-measure-before-tag.md` — **the benchmarks run again** before the tag, on the baseline's
   machine: one regression found and fixed (a 200 000-entry rescan 139 → 118 ms), the per-row
   thumbnail stat found not worth a cache, a new baseline, the run on the release checklist.
   **Built 2026-10-02.**
8. `08-ci-once.md` — **CI builds once**: one run read with a stopwatch; the tests on the release
   profile under makepkg's own flags, which is what had every push compiling twice; the suite
   in two halves on two machines; the release's empty-version commit message. **Built
   2026-10-02**, unverified on Actions until the next push.
9. `09-loose-ends.md` — the fail marker's day as a setting, the demo folders in the home, the
   0.2 video flow, a re-composite flag for the recorder.
10. `10-faster-listings.md` — **the first screenful comes with the open**: rows ride on the
    `Open` reply or the `Reset`, the window never asks a `Window` for them, every `Reset`
    carries the rows on screen. Local and remote. Measured first.
11. `11-chooser-window.md` — **the chooser is a window of its own**, a layer surface on the
    overlay layer, so another application's Save is above the application that asked rather than
    behind it, and on the output the person is looking at. Driven under `sway` locally, where a
    screenshot says it is over that application and gone after an answer; `cage` has no layer
    shell, so those checks are local and not CI's. **Built 2026-10-04.**
12. `12-hypr-lua.md` — **kiki's Hyprland block is Lua**: Hyprland's configuration on Omarchy is
    Lua now and the file kiki wrote is read by nothing, so `Super+Shift+F` had never once opened
    kiki. The block goes into `bindings.lua`, unbinds the chords Omarchy's defaults hold before
    taking them, and starts kiki the way Omarchy starts everything else; the dead block in the
    old file is swept. **Built 2026-10-06.**
13. `13-one-switch.md` — **one switch, and what it does in a person's words**: Settings → Omarchy
    was four rows of Apply/Remove with the file each one writes, and the first-run dialog four
    checkboxes with their paths beneath. Both are now one answer over one list — the keys,
    folders, the Open and Save dialogs, "Show in folder", Quick Look — with no paths and no
    taking three of the four. **Built 2026-10-06.**
