# 03 — Inspector and thumbnails

**Status:** built and tested; Created field not in 0.1.0; the name is read-only by decision (rename lives in the views).

Builds on: `02-shell-and-views.md`. Hooks for later plans: ~~the name field becomes editable and~~ Apply on the Permissions tab becomes active when plan 04 lands; the archive member preview appears with plan 05. Until then the ~~name is read-only, the~~ Permissions tab is read-only, and archives show the kind icon. **Amended 2026-09-21 (D7):** the name stays read-only for good — renaming is done in the views (`F2`), and a second place to do it is a second thing to keep right.

Mockups: the fourth column in `Main.dc.html`, the right panel in `IconView.dc.html` and `ListView.dc.html`, and `InspectorPermissions.dc.html`.

## Goal

One inspector component that is the last column in columns view and a 300 px right panel in icon and list views, toggled from the toolbar. The same thumbnail pipeline that feeds its preview also replaces kind icons with thumbnails for images and video in the icon, list and columns views.

**Width**: it is never narrower than its Permissions grid — `Inspector.minWidth` (88 + 3 × 56 and the margins, 292 px) is the floor for the window's panel and for columns view's info column alike, which at 240 and 260 px cut the grid's last column off (owner, 2026-09-21).

**Where its values come from** (2026-09-21): the listing's row, and only that. It used to ask the daemon to `Stat` a URI whose row had no metadata yet and showed nothing the row would not have shown; `Stat` is left to the transfer code now, and `tst_InspectorPermissions` checks nothing is asked. (Worth knowing: `Stat` on a *remote directory* answers `NotFound`, on both plugins.)

## Design

**Tabs**: General and Permissions ~~in this plan; plan 13 adds a Code tab that becomes the default for text files~~. **Struck 2026-09-21 (D1):** kiki has no built-in code viewer and will not grow one — `CodeTab.qml` and the daemon half behind it are deleted; code is read in an editor (plans 13, 14).

**Header**: kind icon, the name (read-only, D7), path in a mono box.

**General tab**:
- Preview: text (first 40 lines, from `read`, capped at 64 KB), images (scaled, decoded off the UI thread), video (a frame at 10 percent of the duration, with duration and resolution in the fields below), PDF first page (rendered by spawning `pdftoppm` from poppler, like ffmpeg for video, into the thumbnail cache at 256 px and returned as a path; the kind icon if rendering fails or poppler is absent), folders (child names), archives (member list, from plan 05).
- Fields: Type, Host (`local` or the location name), Location, Size (bytes and human), Modified, ~~Created (from `statx` birth time when available)~~ (**not in 0.1.0**, D7: `Meta` carries no birth time), Owner, Group, and Git (state, branch and last commit from plan 15; only shown inside a repository).
- Actions: Open, Open with… (`xdg-open` and the mime handler list).

**Permissions tab**: read / write / execute for owner, group, world; octal and symbolic mirrors that update as boxes change; Apply to contained items; Apply and Revert. Apply submits a `Chmod` job (plan 04), so it is undoable. Disabled on backends whose capabilities lack `mode`.

**Thumbnails**: kikid's `thumbs` module produces 128 px thumbnails for the views and 256 px only when the inspector asks. For JPEGs it first tries the **embedded EXIF thumbnail** (a few KB read, no decode; most camera and phone photos have one) and only falls back to decoding the image; decoding uses the `image` crate with EXIF orientation, and large sources are decoded at reduced scale where the format allows and video (a frame extracted by spawning `ffmpeg` with `-ss` before `-i` so it seeks without decoding, `-frames:v 1`, `-vf scale`, one process at a time per core, lowest priority; a persistent `ffmpeg` per worker is measured as a follow-up if spawn cost shows). They are written to `~/.cache/thumbnails/{normal,large}/` per the freedesktop thumbnail spec, keyed by the file URI's MD5 with the source mtime recorded, so they are shared with other apps and never regenerated for an unchanged file. Generation is viewport-driven like the rest of phase 2 (plan 01): only rows inside the padded viewport window are queued, nearest first, and leaving the window cancels pending work. Rows show the kind icon until their thumbnail lands; the daemon then sends `RowsChanged` and the refetched window row carries `thumb: <path>`, which the delegate loads asynchronously. Files that cannot be thumbnailed are recorded in the spec's `fail/kiki/` directory so they are not retried on every scroll. kiki does not prune the cache; the spec's age-based cleaner convention applies. ~~Remote files are thumbnailed only after they are read for another reason, never speculatively.~~

**Amended 2026-09-21, three ways** (plan 31, phases 4b and the 2026-09-20 list):
- **The decoding is its own process** — `kiki-thumber`, speaking the plugin framing. Thumbnailing is the only place kiki decodes a file's contents, so a malformed file now takes down one child that is started again rather than every listing and every job. The pixels never cross the pipe: the child writes the cache file and the failure marker and answers with a path; a cache hit never leaves the daemon; a child stuck 20 s on one file is killed, and a file that kills it twice is given up on. `preview.rs` goes the same way.
- **Thumbnails are `file://` only in 0.1.0** (owner, 2026-09-20). A remote picture shows its kind artwork — a *cached* one is still served, so nothing already made is lost — and a remote listing queues no jobs at all. It had never worked: the URI's scheme and host were dropped, so the daemon thumbnailed a remote picture from **this machine's** copy of that path, writing fail markers against remote names.
- **Only what is on screen is asked for**: a window holds a viewport plus its look-ahead, and asking for the whole held range spawned a `pdftoppm` or an `ffmpeg` for rows nobody was looking at. `Window` carries `viewFirst`/`viewCount`, and a queued thumbnail nobody can see any more is dropped unstarted — a drop makes the row ask afresh, where "there is no thumbnail for this file" does not.

**Protocol**: `Preview { uri } -> { text } | { path } | { children } | { members }` (text head, or a path into the cache for image, video frame and PDF page), `Thumbnail { uri, size } -> { path }` (also filled into window rows automatically for rows in a window), `GitStatus { uri }` (shape in plan 15). All reads; none are jobs. The inspector sends no `Stat` (above).

## Verification

- Selecting a file in columns view swaps the next column for the inspector in the same `state()` snapshot as the selection change; selecting a folder swaps it back.
- Toggling the inspector in icon and list views resizes the grid and table without losing selection.
- Preview of a 2 GB text file: the `Preview` reply reports bytes read, and it is at most 64 KB.
- Octal and symbolic values agree with `stat -c '%a %A'` for a set of test files.
- A folder of 500 photos and 20 videos: the listing paints before any thumbnail exists; every thumbnail appears within 10 s on a warm cache miss and immediately on a hit; files in `~/.cache/thumbnails/normal/` validate against the spec (`Thumb::URI`, `Thumb::MTime` PNG keys).
- Scrolling that folder in icon view stays under 16 ms per frame while thumbnails are still generating.
