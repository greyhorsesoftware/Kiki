# 27 — Gallery view

Builds on: `03-inspector.md` (previews, the `Preview` and `Thumbnail` requests), `21-view-memory-and-columns.md` (per-folder memory), `23-ui-refinement.md` (the view menu), `24-mirror-view.md` (view as the one axis), `13-code-viewer-and-editor.md` (a full-area reader inside the pane).

## Goal

A picture folder answers one question: *what is in these files?* Icon view answers it badly — a grid of 44px glyphs where every photo is a smudge. Gallery view is the fifth view: **one image filling the pane, a filmstrip of its neighbours along the bottom**. Arrow keys walk the folder, the filmstrip follows, and nothing about the folder changes — same listing, same sort, same selection, so leaving the view puts you back where you were.

It is a *view*, not a separate window: the toolbar, path and shortcut bar stay, `Ctrl+5` and the view menu switch to it, and per-folder memory remembers it like any other.

## Behaviour

- **Entering**: `Ctrl+5`, the view menu, or `Enter` on an image in any view. A folder with no memory that plan 24's rule calls a picture folder — named Pictures, Photos, DCIM, Screenshots, Wallpapers, Camera or Camera Roll, or at least 60 percent images and video — opens straight into Gallery. Memory still wins: a folder you have put in another view stays there.
- **The stage** is the pane minus the filmstrip: the current image, scaled to fit, centred, on the pane background. Never upscaled past 100 percent at fit; `0` fits, `1` shows 100 percent, `+`/`-` step, pinch or `Ctrl`+wheel zooms (plan's icon-view gesture, same clamps), drag pans once zoomed.
- **The filmstrip** is a horizontal row of thumbnails along the bottom, 72px tall, the current one ringed in the accent. It scrolls to keep the current item centred, is itself scrollable with a two-finger sideways swipe, and a click jumps to that image.
- **What it contains**: every row of the current listing, in the listing's order, honouring the filter — pictures as thumbnails, anything else as its kind icon. Keeping the strip index equal to the listing index is what lets the selection, the inspector and the shortcut bar carry on working unchanged. A video shows its poster frame with a play badge and opens externally on `Enter`. Other kinds are skipped — the count in the shortcut bar reads `7 of 31 images` so it is clear the folder holds more.
- **Empty case**: a folder with no images shows the kind icon and "No pictures here", and the view switcher still offers the other four.

## Keys

| Key | Action |
|---|---|
| `←` `→` | previous / next image (this is the one place `←` does not leave the folder) |
| `Home` `End` | first / last |
| `Space` | next |
| `Enter` | open in the default application |
| `0` `1` `+` `-` | fit, 100 percent, zoom in, zoom out |
| `F` | hide or show the filmstrip |
| `Ctrl+I` | info panel for the current image |
| `Del` | move to trash and advance to the next image |
| `Esc` | back to the view you came from (Icon when the folder opened straight into Gallery) |

Plan 23's type-ahead is off in this view: single letters are free, so `F` and the zoom keys need no modifier.

## Loading

- **Local files** load straight from `file://` with `sourceSize` capped to the stage size, so a 48MP photo is decoded once at display size rather than in full.
- **Remote files** (plan 06 locations) cannot be handed to the image loader as a path. The stage asks the daemon for a large preview — `Preview` already returns a cached `Thumbnail` at `Size::Large` — and shows that, with a `full size unavailable offline` note when the large render is itself a downscale. Opening externally still fetches the original.
- **Neighbours**: the stage prefetches the next and previous two images at stage size and keeps a small MRU of decoded images, so paging is instant and memory stays bounded. The filmstrip uses the existing 128px thumbnails and the daemon's live-window generation, no new request.
- **Failures** show the kind icon and the reason, exactly as the inspector's preview box does; a corrupt file never blocks paging.

## Daemon

Nothing new is required. The view uses `Window`/`Rows` for the listing it is given, `Thumbnail` for the filmstrip and `Preview` for remote stages. One optional addition, if paging a slow location feels thin: `Prefetch` accepting a row range so the daemon can warm several large thumbnails ahead of the stage rather than one at a time.

## Settings

One row under General: **Filmstrip** — `bottom` (default) or `hidden`, remembered per folder with the view. No slideshow, no transitions, no EXIF panel: the inspector already shows metadata on `Ctrl+I`.

## Shell

- `qml/kiki/views/GalleryPane.qml` — stage plus filmstrip, the same `pane` interface as the other views (`ensureVisible`, `perRow`, `pageSize`) so `Shell.qml` routes keys to it unchanged.
- `Shell.qml` — a fifth entry in the view switcher and `Ctrl+5`; `enterSelected()` on an image switches to Gallery rather than launching it.
- `ViewSwitcher.qml` — a fifth icon (`image`).

## Acceptance

- A folder of 500 photos opens, pages with `←`/`→` at a steady frame rate, and never decodes an image larger than the stage.
- Switching to List and back leaves the same file selected, scrolled into view.
- Filtering to `*.png` narrows the filmstrip and the count without leaving the view.
- A folder on an SFTP location pages without stalling the shell; images that are not cached show a progress state, not a blank stage.
- Trashing the last image leaves the view on the new last image, or the empty state when the folder runs out.
