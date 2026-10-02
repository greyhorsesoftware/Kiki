# API delta — 0.5.0

What changed in the daemon's protocol since 0.4.1. A window and its daemon are the same build
by construction (the socket is named for the version and `Hello` refuses any other), so nothing
here is kept as an alias. The full tables are in `docs/0.1.0/API-DAEMON.md`.

## The first screenful comes with the open (`10-faster-listings.md`)

- **`Open`** takes `initial?: u32` — how many rows from the top the window wants without
  asking (the window sends its viewport plus look-ahead, 260). When the daemon already has the
  folder listed, the reply carries them in the shape a `Window` answers in:
  `{ cached, first: 0, rows, n, done, gen }`. When it has not, the reply is `{ cached }` as
  before and the rows come on the `Reset` below. No `initial`, or `0`: nothing rides along,
  the reply is `{ cached }`, and the client asks a `Window` as it always did. `view?: u32`
  beside it says how many of those rows are on screen from the top (the window sends its
  viewport, 60): thumbnails are made for those alone, as `Window`'s `viewCount` has always
  meant; unsaid, for all of `initial`.
- **`Reset`** carries `first` and `rows` — the rows the connection holds (its live window, or
  the first `initial` of its `Open` until it has asked one) in their new places. Every `Reset`:
  the end of a scan, a `Sort`, a `Filter`, a `ShowHidden`, a `Refresh`, a watched folder that
  changed too much to splice. Local rows are stated before they go, a screenful at most and
  within 20 ms; the rest arrive by `Rows` as before. A connection whose `Open` named no
  `initial` and has asked no `Window` gets a `Reset` with neither field.
- **`Sort`** of a folder still being listed sends no `Reset`: the order is noted and the scan's
  own `Reset` applies it. (It used to send one with nothing in it, and the window answered with
  a `Window` request — one of the three a cold open made.)

## A drag across mountpoints is a copy (`01-ui-cleanup.md`, item 3)

- **`Open`**'s reply carries `device?: u64` — the local folder's `st_dev`; absent for a
  server's folder. The window compares the dragged items' with the target's: different
  devices are different places, and the drop is a copy with a + rather than a move the daemon
  would do as a copy and a delete.

## The daemon's bounds (`02-daemon-bounds.md`)

- **Error 1273** — "gave up after an hour: it was still not finished": a plugin request that
  ran past `REQUEST_CEILING` (an hour, moving or not — `REQUEST_TIMEOUT` is the patience
  between frames and goes on being that), or a `bsdtar` that did. The three catalogs have it.
- **A window that stops reading its socket is let go.** The daemon keeps at most 4 096 frames
  for a connection; past that, a frame a later one supersedes (`Progress`, a `Count` still
  growing, a `JobEvent` still running) is dropped oldest-first to make room, and a frame that
  must arrive that finds no room within a second closes the connection. A window reads every
  frame and never sees this; one that has stopped reconnects (`Daemon.qml`) and resubscribes.
  `kikid.log` says how many frames were dropped, once, when the connection ends.

## The index keeps up (`06-index-live.md`)

- **`IndexStatus`** gains `live: u64` — entries a watched folder's changes put into the index
  since the last refresh or build. Nothing else on the wire: a file made in a folder a window
  is showing is in `Search` (scope `everywhere`) within a second, where before it waited for
  the ten-minute walk.

Nothing was removed.
