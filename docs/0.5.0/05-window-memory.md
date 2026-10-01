# 05 — The window's memory floor

**Status:** planned, 2026-09-29 — a measurement first, and a fix only for what the measurement
names. `docs/0.1.0/27-gallery-view.md` (2026-09-19): "the window's floor rises with the largest
folder visited: fresh shell 249 MB, +62 MB once a 1 000-row folder has been shown, and it does
not come back" — Qt's JS heap and scene graph, not the row objects. A laptop that browses a
photo library all afternoon ends the day with a file manager the size of a browser.

## What is known

- The daemon is fine: kikid 5 → 26 MB holding a 1 000-file listing, 14 MB back after
  (`mallopt`, measured in the audit); the listing cache is LRU-bounded. This plan is the shell.
- `WindowCache.qml` pads 100 rows ahead and 50 behind and holds row objects as JS values; the
  views are `ListView`/`GridView` delegates with a `cacheBuffer`; the gallery decodes at twice
  the stage size and keeps `Image` items for its neighbours; thumbnails and gallery pictures are `Image`
  items with `cache` on by default, which keeps decoded pixels in Qt Quick's own pixmap cache
  — how large that is allowed to grow, and whether Quickshell sets anything for it, is the
  first thing the measurement establishes rather than something this plan assumes.

## Decisions

1. **Measure before touching.** A flow `tests/e2e/flows/memory_floor.py` (not in the default
   run, like `gallery_perf`): fresh window, RSS; open a 1 000-row folder in list view, RSS;
   leave it for an empty folder, RSS after 5 s and after 30 s; the same for icon view (which
   loads thumbnails) and the gallery; then 5 000 rows. The numbers go into
   `bench/shell-memory.jsonl` with the commit, so the fix is a diff of that file.
2. **The suspects, in the order to try them, each measured alone:**
   - `Image.cache: false` on thumbnail and gallery images, with kiki's own decode window as the
     cache (the thumbnails are files on disk already; a second copy in the pixmap cache is the
     likeliest 60 MB).
   - `gc()` called once when a listing is dropped (`Pane.open` to a different folder), which
     Qt's JS engine will not do on its own until allocation pressure — cheap to test, and the
     measurement says whether the heap is what holds the floor.
   - `WindowCache` rows: are they released when the listing is, or kept by a closure in a
     delegate? `Object.keys` count through an IPC probe.
   - `cacheBuffer` on the views (a screen's worth is the default; the list may have more).
3. **The bar:** the 30 s-after number within 15 MB of the fresh number for list view, within
   30 MB for icons and the gallery (a screen of decoded pictures is legitimately kept). If the
   measurement says the floor is Qt's and none of the four moves it, that is written here and
   the plan closes with the flow kept — the number is then watched, not fought.

## Size

Two days: the flow half a day, the four experiments a day, the write-up the rest. No wire
change. The flow needs the gallery fixture (`KIKI_PERF_DIR`, `kikid bench gen photos`).
