# 10 — The first screenful comes with the open

**Status:** built, 2026-10-01 — L0 to L3 (owner's brief: `~/Downloads/PLAN-PIGGYBACKED-WINDOW-1.md`).
Opening a folder paints its first rows in **one** trip to the daemon: the rows ride on the
message that announces the folder, and the window never asks for them. Local and remote.

Measured by `make open-perf` on the owner's desktop (`KIKI_E2E_DESKTOP=1`, the GPU renderer;
`bench/open-history.jsonl`), the median of five openings each, before and after — "requests"
is the `Window` requests the window made before its first screenful was there to draw, the
number this plan exists to change; "first rows" is when that screenful was there:

| opening | first rows before | after | requests before → after |
|---|---|---|---|
| 1 000 files, cold | 22 ms | 19 ms | 3 → 0 |
| 1 000 files, cached | 8 ms | 10 ms | 1 → 0 |
| 10 000 files, cold | 74 ms | 56 ms | 3 → 0 |
| 10 000 files, cached | 11 ms | 10 ms | 1 → 0 |
| 1 000 files on SFTP, cold | 296 ms | 308 ms | 3 → 0 |
| 1 000 files on SFTP, cached | 8 ms | 10 ms | 1 → 0 |

A cold local open is a quarter faster; cached and remote are within the noise (±2 ms on the
cached ones; a remote open is the server's scan, 300 ms of it, and the round trip was a
hundredth of that). Under `cage`'s software renderer, where the first run was read, nothing
moved: there the window's own handling of the scan's `Count` events costs a hundred
milliseconds a `done` the daemon sent at 5 ms, and that spread swallows the twenty this saves
— plan 05's and 07's question. Two things found on the way, both by the desktop measurement,
both now in: the `Count` with the `Open` reply left **before** the rows were built, so the
window laid out a screenful of empty rows in the three milliseconds it waited and filled them
in after (a cached open twice as long as the trip it had saved); and the first screenful's
thumbnails were asked for the whole of `initial` (260 rows) rather than the 60 on screen —
four times the decoders a cold open of a picture folder used to start, and a slower paint
(`view` on `Open`, below). The third request a cold open used to make was a `Sort`'s `Reset`
with nothing in it.

## Today

`WindowCache.open` sends `Open` and a `Window(0, 260)` in one write. The daemon starts the
scan on a thread and answers that `Window` at once with what it has — a millisecond into a
cold scan, nothing, or a partial view in readdir order (the sort is `rebuild_view`, at the end
of the scan). When the scan ends the daemon sends `Reset`; the window marks itself stale,
throws the rows away and asks for the `Window` **again** (`WindowCache.qml`, `case "Reset"` →
`_refetch()`). Those rows then arrive with names and no sizes (local `meta` is `None` until the
stat pool has run), and `Rows` events fill them in. So a cold open is: a wasted `Window`, a
`Reset`, a second `Window`, then the fill — three paints where one would do. A cached open
(`cached: true`) has no scan and no `Reset`: the one `Window` answers in full.

Remote listings already carry their metadata in the scan (`e.meta` from SFTP's readdir, FTPS's
`MLSD`, gio's `meta_in_scan`), so for them the second trip buys nothing at all.

## Decisions

1. **Whichever message says how many rows there are says what the first ones are.** `Open`
   gains `initial: u32` (what the window would like: `viewportCount + padAhead`, 260 by
   default; 0 for none). A listing whose scan is already done answers `Open` with
   `{ cached, first: 0, rows: [Row] }`; one that is still scanning answers `{ cached }` and the
   `Reset` at the end of the scan carries `first` and `rows`. The window sends no `Window`
   with its `Open` any more.
2. **Every `Reset` carries rows, not just the first.** `Sort`, `Filter`, `ShowHidden`,
   `Refresh` and a rescan all `Reset` and all re-request today; each now carries the
   subscriber's *current* window (`s.first`, `s.count` — `0..initial` before any `Window` has
   set them), so a sort repaints in one trip at the rows that were on screen.
3. **The rows are whole.** Before sending, the daemon stats what in that range has no `meta`
   (local only — remote has it), the lock dropped while it does, as `run_stats` does, and the
   epoch re-checked after; at most 128 rows and at most 20 ms, then what is stated goes and
   the rest fills by `Rows` as now. Warm, 128 `statx` is under a millisecond; a cold disk or a
   network mount does not hold the first frame hostage.
4. **The window applies before it trusts.** On `Reset` with rows: `count`, `gen`, the rows
   into `_rows`, `reset()` then `rowsUpdated(first, n)`, **then** `_stale = false` and `_fill()`
   for anything the viewport wants beyond them. A `Reset` without rows (an error, `initial: 0`)
   stays stale and refetches as today. An `Open` reply with rows is applied the same way.
   Thumbnails for the rows on screen are asked for as `window()` asks today (the `view`
   covers them).

## The wire (`API-DELTA.md`)

| | before | after |
|---|---|---|
| `Open` | `{ lid, uri }` → `{ cached }` | `{ lid, uri, initial?, view? }` → `{ cached, first?, rows? }` (rows when the scan is done; `view` is how many of them are on screen) |
| `Reset` | `{ lid, n, gen }` | `{ lid, n, gen, first?, rows? }` |

`Window` is unchanged and still what a scroll asks with.

## Levels

- **L0 — measure** (half a day). `tests/e2e/flows/open_perf.py`, not in the default run: open →
  first painted row, through the window (the view's first delegate with a non-empty name,
  read by IPC), for a local folder cold and cached, 10 000 entries, and an SFTP folder from
  `servers.py`; also the count of `Window` requests the open made (`KIKI_TRACE`). Numbers into
  `bench/open-history.jsonl` with the commit, before and after.
- **L1 — the daemon** (a day). `initial` on `Subscriber`; `handlers/listing.rs open()` answers
  with rows when `scan_done`; `scan.rs` builds the rows after `rebuild_view` and sends them
  on `Reset`; the other `Reset` sites (`scan.rs:125, 298, 397`) go through one
  `reset_with_rows(subs)` so there is one place that does it. Tests: an `Open` on a cached
  listing answers rows; a cold `Open` answers none and its `Reset` carries `initial` rows with
  `meta` set; `Sort` on an open listing `Reset`s with the subscriber's window; `initial: 0`
  carries nothing; an empty folder carries `rows: []`; a scan error carries nothing.
- **L2 — the window** (half a day). `WindowCache.open` sends `initial` and no `Window`;
  `handleEvent("Reset")` and the `Open` callback apply rows per decision 4. QML tests against
  the fake daemon: no `Window` leaves with an `Open`; rows on the `Open` reply paint; rows on
  `Reset` paint and the stale `Window` reply that was in flight is dropped by `gen`; a
  scroll past the piggybacked range sends a `Window` as before.
- **L3 — the bounded stat** (half a day): decision 3 with its test (a `Dir` stand-in whose
  `stat_child` sleeps: the `Reset` leaves within the bound with the rows it has).

## Verification

L0's flow run before L1 and after L3 — the `Window`-per-open count reads 0, the first-paint
time is read and written into this status line — then the full gate, and `API-DAEMON.md`'s
`Open` and `Reset` rows updated beside `API-DELTA.md`.
