# 03 — Inspector and thumbnails

Builds on: `02-shell-and-views.md`. Hooks for later plans: the name field becomes editable and Apply on the Permissions tab becomes active when plan 04 lands; the archive member preview appears with plan 05. Until then the name is read-only, the Permissions tab is read-only, and archives show the kind icon.

Mockups: the fourth column in `Main.dc.html`, the right panel in `IconView.dc.html` and `ListView.dc.html`, and `InspectorPermissions.dc.html`.

## Goal

One inspector component that is the last column in columns view and a 300 px right panel in icon and list views, toggled from the toolbar. The same thumbnail pipeline that feeds its preview also replaces kind icons with thumbnails for images and video in the icon, list and columns views.

## Design

**Tabs**: General and Permissions in this plan; plan 13 adds a Code tab that becomes the default for text files.

**Header**: kind icon, editable name (commits a rename job from plan 04), path in a mono box.

**General tab**:
- Preview: text (first 40 lines, from `read`, capped at 64 KB), images (scaled, decoded off the UI thread), video (a frame at 10 percent of the duration, with duration and resolution in the fields below), PDF first page (rendered by kikid with `pdfium-render` into the thumbnail cache at 256 px and returned as a path; the kind icon if rendering fails), folders (child names), archives (member list, from plan 05).
- Fields: Type, Host (`local` or the location name), Location, Size (bytes and human), Modified, Created (from `statx` birth time when available), Owner, Group, and Git (state, branch and last commit from plan 15; only shown inside a repository).
- Actions: Open, Open with… (`xdg-open` and the mime handler list).

**Permissions tab**: read / write / execute for owner, group, world; octal and symbolic mirrors that update as boxes change; Apply to contained items; Apply and Revert. Apply submits a `Chmod` job (plan 04), so it is undoable. Disabled on backends whose capabilities lack `mode`.

**Thumbnails**: kikid's `thumbs` module produces 128 px thumbnails for the views and 256 px only when the inspector asks. For JPEGs it first tries the **embedded EXIF thumbnail** (a few KB read, no decode; most camera and phone photos have one) and only falls back to decoding the image; decoding uses the `image` crate with EXIF orientation, and large sources are decoded at reduced scale where the format allows and video (a frame extracted by spawning `ffmpeg` with `-ss` before `-i` so it seeks without decoding, `-frames:v 1`, `-vf scale`, one process at a time per core, lowest priority; a persistent `ffmpeg` per worker is measured as a follow-up if spawn cost shows). They are written to `~/.cache/thumbnails/{normal,large}/` per the freedesktop thumbnail spec, keyed by the file URI's MD5 with the source mtime recorded, so they are shared with other apps and never regenerated for an unchanged file. Generation is viewport-driven like the rest of phase 2 (plan 01): only rows inside the padded viewport window are queued, nearest first, and leaving the window cancels pending work. Rows show the kind icon until their thumbnail lands; the daemon then sends `RowsChanged` and the refetched window row carries `thumb: <path>`, which the delegate loads asynchronously. Files that cannot be thumbnailed are recorded in the spec's `fail/kiki/` directory so they are not retried on every scroll. kiki does not prune the cache; the spec's age-based cleaner convention applies. Remote files are thumbnailed only after they are read for another reason, never speculatively.

**Protocol**: `Preview { uri } -> { text } | { path } | { children } | { members }` (text head, or a path into the cache for image, video frame and PDF page), `Thumbnail { uri, size } -> { path }` (also filled into window rows automatically for rows in a window), `GitStatus { uri }` (shape in plan 15). All reads; none are jobs.

## Verification

- Selecting a file in columns view swaps the next column for the inspector in the same `state()` snapshot as the selection change; selecting a folder swaps it back.
- Toggling the inspector in icon and list views resizes the grid and table without losing selection.
- Preview of a 2 GB text file: the `Preview` reply reports bytes read, and it is at most 64 KB.
- Octal and symbolic values agree with `stat -c '%a %A'` for a set of test files.
- A folder of 500 photos and 20 videos: the listing paints before any thumbnail exists; every thumbnail appears within 10 s on a warm cache miss and immediately on a hit; files in `~/.cache/thumbnails/normal/` validate against the spec (`Thumb::URI`, `Thumb::MTime` PNG keys).
- Scrolling that folder in icon view stays under 16 ms per frame while thumbnails are still generating.
