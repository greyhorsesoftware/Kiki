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

Nothing was removed.
