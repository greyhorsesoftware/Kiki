# 27 — Gallery view

**Status:** built and tested; `Ctrl`+wheel zoom, the video poster frame, the remote large preview and the Filmstrip setting are not in 0.1.0 (D6). A slideshow and a cross-fade, which this plan ruled out, are in.

Builds on: `03-inspector.md` (previews, the `Preview` and `Thumbnail` requests), `21-view-memory-and-columns.md` (per-folder memory), `23-ui-refinement.md` (the view menu), `24-mirror-view.md` (the view-by-contents rule). ~~`13-code-viewer-and-editor.md` (a full-area reader inside the pane)~~ — there is no built-in reader; plan 13 is the editor bridge (D1).

## Goal

A picture folder answers one question: *what is in these files?* Icon view answers it badly — a grid of 44px glyphs where every photo is a smudge. Gallery view is the fourth view (it was the fifth until Side by Side left the menu): **one image filling the pane, a filmstrip of its neighbours along the bottom**. Arrow keys walk the folder, the filmstrip follows, and nothing about the folder changes — same listing, same sort, same selection, so leaving the view puts you back where you were.

It is a *view*, not a separate window: the toolbar, path and shortcut bar stay, `Ctrl+5` and the view menu switch to it, and per-folder memory remembers it like any other.

## Behaviour

- **Entering**: `Ctrl+5`, the view menu, or `Enter` on an image in any view. A folder with no memory that plan 24's rule calls a picture folder — named Pictures, Photos, DCIM, Screenshots, Wallpapers, Camera or Camera Roll, or at least 60 percent images and video — opens straight into Gallery. Memory still wins: a folder you have put in another view stays there.
- **The stage** is the pane minus the filmstrip: the current image, scaled to fit, centred, on the pane background. Never upscaled past 100 percent at fit; `0` fits, `1` shows 100 percent, `+`/`-` step, ~~pinch or `Ctrl`+wheel zooms (plan's icon-view gesture, same clamps)~~ — **not in 0.1.0** (D6): zoom is on the keys, and the wheel walks the filmstrip. Drag pans once zoomed, and the stage starts no drag while there is something to pan (`canPan`).
  - **A small picture is not blown up** (fixed 2026-09-21, and it was one property). `fillMode: PreserveAspectFit` on the two frames turned `sourceSize` from a ceiling into a size to scale **to**: a 120 × 80 PNG came back 1600 px wide, and `1` ("actual size") showed that upscaled decode rather than the file's pixels. Without the property the cap caps — 513 MB against 589 MB over a thousand photographs, and a decode of 11 ms against 14. *Still true*: "actual size" on a picture larger than the cap shows the capped decode.
- **The filmstrip** is a horizontal row of thumbnails along the bottom, 72px tall, the current one ringed in the accent. It scrolls to keep the current item centred, is itself scrollable with a two-finger sideways swipe or the wheel, and a click jumps to that image; a double click on a folder goes into it.
  - **A tile re-reads its row** on a new index, on reuse, on rows updated and on a reset (2026-09-21). The strip recycles its tiles, and a tile held its row as a value from the first `rowsUpdated` on — the assignment had replaced the binding — so a recycled tile went on showing the file of the place it had before: in a folder of mixed kinds, a text file's icon where a picture was. The picture itself was never by position; `thumb` is the daemon's path for that file and travels with the row. `tst_GalleryPane` scrolls 400 mixed rows both ways, and fails without the re-read.
- **What it contains**: every row of the current listing, in the listing's order, honouring the filter — pictures as thumbnails, anything else as its kind icon. Keeping the strip index equal to the listing index is what lets the selection, the inspector and the shortcut bar carry on working unchanged. ~~A video shows its poster frame with a play badge~~ — **not in 0.1.0** (D6): a video shows its kind artwork and opens externally on `Enter`. ~~Other kinds are skipped~~ — the arrows step through **every** entry, which is also all the shell can know (rows are windowed), so the count in the bottom bar reads **`7 of 31`** — where you are among the entries, not how many are selected (`Shell.countText`, and it falls back to `N items` where the gallery has nothing on the stage).
- ~~**Empty case**: a folder with no images shows the kind icon and "No pictures here".~~ **Amended 2026-09-20** (D6, owner): a folder with no pictures is a folder to look through, not an empty state. The stage draws the selected entry with **the artwork Icon view draws** (`UI.KindIcon`), scaled to about two fifths of the stage's shorter side (128–320 px), with the name under it; a thumbnail, where the row has one, still wins. The filmstrip shows every entry the same way, `Enter` / `→` on a folder goes into it and stays in Gallery, and only a folder with nothing in it at all says so — "Empty folder", as the other views do. A **remote** picture shows kind artwork too: thumbnails are `file://` only in 0.1.0 (plan 31, 2026-09-20), and the stage names the file under it.

## Keys

| Key | Action |
|---|---|
| `←` `→` | previous / next entry (~~this is the one place `←` does not leave the folder~~ — **amended 2026-09-20:** `←` on the *first* entry goes up to the parent, deliberately, and is kept) |
| `Home` `End` | first / last |
| `Space` | next |
| `Enter` | open in the default application |
| `0` `1` `+` `-` | fit, 100 percent, zoom in, zoom out |
| `F` | hide or show the filmstrip |
| `Ctrl+I` | info panel for the current image |
| `Del` | move to trash and advance to what followed (`keepPlace`; it used to jump back to the first picture, so deleting through a shoot restarted from the top each time) |
| `Esc` | back to the view you came from (Icon when the folder opened straight into Gallery) |

Plan 23's type-ahead is off in this view: single letters are free, so `F` and the zoom keys need no modifier.

## Loading

- **Local files** load straight from `file://` with `sourceSize` capped to the stage size, so a 48MP photo is decoded once at display size rather than in full.
- **Remote files** (plan 06 locations) cannot be handed to the image loader as a path. ~~The stage asks the daemon for a large preview — `Preview` already returns a cached `Thumbnail` at `Size::Large` — and shows that, with a `full size unavailable offline` note.~~ **Not in 0.1.0** (D6, and the 2026-09-20 decision that thumbnails are `file://` only): the stage shows a *cached* thumbnail where there is one and the file's kind artwork with its name otherwise. Opening externally still fetches the original. Fetching the bytes to render a remote picture — the plugin's `Thumb` hook, or a capped `Read` — is post-0.1.0; the SDK hook is there for it.
- **Neighbours**: the stage prefetches the next and previous two images at stage size and keeps a small MRU of decoded images, so paging is instant and memory stays bounded. The filmstrip uses the existing 128px thumbnails and the daemon's live-window generation, no new request.
- **Failures** show the kind icon and the reason, exactly as the inspector's preview box does; a corrupt file never blocks paging.

## Daemon

Nothing new is required. The view uses `Window`/`Rows` for the listing it is given and `Thumbnail` for the filmstrip; it tells the daemon which rows the strip is showing (`setViewport`) so those are the thumbnails made. ~~`Preview` for remote stages. One optional addition … `Prefetch` accepting a row range.~~ Neither is in 0.1.0: remote stages are above, and `Prefetch` is struck from plan 01 altogether (D9 — the daemon had it and nothing ever sent it).

## Settings

~~One row under General: **Filmstrip** — `bottom` (default) or `hidden`, remembered per folder with the view.~~ **Not built** (D6): the filmstrip is on and `F` hides it for as long as the view is up; there is no setting and it is not remembered. ~~No slideshow, no transitions~~ — both are in, below. No EXIF panel: the inspector already shows metadata on `Ctrl+I`.

## Shell

- `qml/kiki/views/GalleryPane.qml` — stage plus filmstrip, the same `pane` interface as the other views (`ensureVisible`, `perRow`, `pageSize`) so `Shell.qml` routes keys to it unchanged.
- `Shell.qml` — an entry in the view menu and `Ctrl+5`; `enterSelected()` on an image switches to Gallery rather than launching it, and `Esc` goes back to the view it came from.
- `ViewSwitcher.qml` — the `image` icon. (The menu is four views since Side by Side left it — plan 29 J.)

## Acceptance

- A folder of 500 photos opens, pages with `←`/`→` at a steady frame rate, and never decodes an image larger than the stage.
- Switching to List and back leaves the same file selected, scrolled into view.
- Filtering to `*.png` narrows the filmstrip and the count without leaving the view.
- A folder on an SFTP location pages without stalling the shell; a picture with no cached thumbnail shows its kind artwork and its name, not a blank stage.
- Trashing the last image leaves the view on the new last image, or the empty state when the folder runs out.
- The gallery is a drag source: the picture on the stage and each filmstrip tile, a selection leaving whole, and a zoomed picture panning rather than starting a drag (`tst_GalleryPane`, plan 31 phase 5).

## Slideshow and the fade (added after the first pass)

The plan above said "no slideshow, no transitions". Both are in: play/pause and a settings popover
(delay 2/4/8/15 seconds, Loop) on a pill under the stage, and a cross-fade between pictures.

The fade is two `Image` frames that swap roles rather than one picture reloaded twice. The next
photograph decodes in whichever frame is not showing and fades in over the one that is, which keeps
its pixels until the fade ends — a single frame would have to start the fade from an empty stage,
which is no fade at all. `bFront` says which frame is in front; `z` follows it so the incoming one
is always on top.

Two traps, both found the hard way:

- `this` in a signal handler is not the object. `onStatusChanged: root.arrived(this)` silently does
  nothing; the frame has to name itself.
- Sibling bindings do not settle in a known order. `onSourceChanged` read `isImage`, which comes off
  the same `row` and had not caught up, so the picture was skipped. `wantPicture()` works it out on
  the spot instead.

## Under load: a thousand photographs

`tests/e2e/run.sh --flow gallery_perf` builds a fixture of 1,000 JPEGs at 1600×1200
(`kikid bench gen gallery1k`, kept at `/tmp/kiki-perf/gallery1k` between runs) and times each part of
showing them. `galleryStats` over IPC reports what the window itself measured — decodes, average,
worst — so the figures are not just the driver's own round trips.

Measured on Omarchy, x86_64, under cage (software rendering); the live session is the same or better:

| What | Cost |
| --- | --- |
| Listing 1,000 files, daemon side | 1 ms to first rows, 52 ms to the full count |
| The same through the window | ~160 ms, including one 22 ms ipc round trip |
| First picture on screen after switching to Gallery | ~65 ms, 16 ms of it decoding |
| Stepping to the next picture | 12 ms decode; ~52 ms wall, most of which is two ipc round trips |
| Jumping to row 500 | ~51 ms |
| Thumbnails, cold, 4 niced workers | ~338/s |
| kikid holding the listing | 5 MB → 26 MB |

Nothing in the picture path is a bottleneck: `sourceSize` caps every decode at twice the stage, so a
16 MP photograph costs the same as a 2 MP one, and 40 steps in a row move the window's resident set
by a megabyte — the two frames are the whole budget.

What the run did find:

- **The window's floor rises with the largest folder visited.** Fresh shell 249 MB, +62 MB once a
  1,000-row listing is held, +27 MB for icon and gallery views; leaving for a small folder gives back
  about 10 MB and no more. The row objects are a fraction of that — it is Qt's JS heap and scene
  graph, and it does not come back.
- **`select` by name could not reach a row outside the loaded window.** The cache keeps a few hundred
  rows either side of the viewport, so scanning `listing.row(i)` found nothing and the call did
  nothing at all — silently. It now falls back to the daemon's `SeekName`, which is what
  `selectCameFrom` already used.
- **The filmstrip asks for a screenful at a time**, which is right, but means "thumbnails per second
  while browsing" measures demand, not the pipeline. The ceiling is measured through the daemon.
