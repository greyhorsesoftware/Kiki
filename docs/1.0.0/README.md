# 1.0.0

The plans for the release after 0.6.0. Sizes are in each.

1. `01-from-the-fork.md` — **what the fork found, and the cycle's own two.** Five commits from
   [ChrisTitusTech/Path](https://github.com/ChrisTitusTech/Path), a renamed fork of kiki, read
   for what was worth bringing back: three real holes — a filename interpreted as Neovim
   keystrokes, untrusted listing names joined to a local root, an SFTP password offered to a
   host nobody has verified — one live test-suite race kiki shared, two CI conveniences, and a
   dependency upgrade. With the version bump that opens the cycle and the About dialog's
   tagline, nine items. **Planned 2026-10-24; all nine built the same day.**

   1. the version, now `1.0.0` · 2. the Neovim filename-as-keystrokes hole · 3. untrusted names
   joined to a local root · 4. permissions and times through a staged copy (needed no change) ·
   5. SSH refusing before it authenticates · 6. russh 0.52 → 0.64.1 · 7. CI by hand, and not
   releasing a hand-run revision · 8. the parallel-test collision · 9. the About line.

2. `API-DELTA.md` — the 1.0.0 protocol changes, for whoever reads the wire. Short this release:
   nothing in the window's protocol moved, and both entries are the plugin contract — what a
   `Scan` entry's `name` and `rel` may be, now that a listing which breaks it fails the scan
   rather than losing an entry; and `Connect` with an unknown host key, which did not change at
   all, the SFTP plugin having simply not been keeping it. `docs/0.1.0/API-PLUGIN.md` carries
   both, and one line of it that described a filter which never existed is now a rule that does.

`ignored.md` is not a plan and holds nothing to build: it is what the fork changed and kiki
declined, with the reason — one commit that patches code kiki had already deleted, and five
lines inside commits kiki *did* take that read like part of the fix and are not. It is the
reasoning behind five decisions in items 2, 3, 4, 6 and 8, kept so that a later reading of those
commits does not undo them.

**Before a `v1.0.0` tag:** item 1 recorded a gate — the three security items done before the
tag — and all three are. The benchmark run is done too (2026-10-24, in item 3): no regressions
beyond 25 %, `copy_file_range` confirmed intact at ~3.2 GB/s, and the scan *faster* despite
checking every one of 202 000 names. What is left is a new baseline recorded at the tagged build,
and item 7's CI changes, which parse but cannot be proven until a push.
