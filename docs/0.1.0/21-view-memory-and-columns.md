# 21 — View memory per folder and list columns

**Status:** built and tested.

Builds on: `01-daemon-and-listing.md` (`Meta`, sort roles), `02-shell-and-views.md` (panes, list view), `20-settings.md` (General page).

## Goal

A folder opens the way you last looked at it, and the list view shows the columns you chose. The Accessed column that plan 22 adds is one of those columns.

## View memory per folder

- **What is remembered**: view mode (icon, list, columns, **gallery** — plan 27), sort role and sort order, and whether hidden files are shown (plan 23), per exact folder. Nothing is inherited by subfolders: a photo folder's icon view should not leak into its `raw` subfolder.
- **Keying**: by URI with the trailing slash stripped, so local, remote (`sftp://name/path`) and `trash:///` listings all work the same way.
- **Storage**: `~/.config/kiki/views.toml`, `[[folder]] uri, view, sort, order, hidden, at`. Capped at the most recent 1,000 folders by `at` (the last time the entry was set); older entries fall off.
- **Behaviour**: `Pane.open` looks the folder up and applies the entry, else keeps the current view and the global default sort. Changing the view (the view menu, `Ctrl+1/2/3/5`) or the sort (header click) writes the entry immediately. A second window picks changes up from the `ViewPrefsChanged` event.
- **Settings → General**: "Remember view per folder" (on by default) and "Forget all". With the switch off, nothing is read or written and every folder uses the defaults.
- **No memory, and it looks like pictures**: the pane opens it in **Gallery** — by name, or because at least sixty per cent of the first two hundred rows are images or video, twelve rows at least (`Pane._smart`, `tst_PaneSmartView`). Memory wins over the guess, and the guess is **not written back** as the folder's choice: it is what the pane does in the absence of one, not a decision the user made. (Plan 24 said Icon; the code is right — D18.)
- **Side by Side is off** for all of this (plan 29 J): both panes start in List, `Pane.rememberViews` is false while the layout is on, so nothing is looked up, nothing is guessed and nothing is written — a folder remembered as Gallery must not open that way in half a window. Leaving puts the remaining pane back on the folder's remembered view.
- An entry left by an older build with `view = "mirror"` loses its `view` on load and keeps its sort and hidden-files choice; the file is rewritten once and never again (`config::drop_mirror_views`).
- **Protocol**: `ViewPrefs -> { folders: { uri: { view, sort, order, hidden? } } }`, `SetViewPref { uri, view, sort, order, hidden? }`, `ClearViewPrefs`, event `ViewPrefsChanged { uri? }`.

## List columns

- Name is always first. **Modified**, **Size**, **Kind** and **Accessed** are each optional; the set lives in `settings.toml [view] columns` (default `["mtime", "size", "kind"]`) and is edited on the General page with one switch per column. The list header and rows are built from that list, so column widths and the rename overlay follow it.
- **Accessed** is the access heat map column; its rendering, data source and the `relatime` caveat are plan 22.
- **Widths are dragged** (added 2026-09-21). The edge a value column *begins* at is its grip — the value columns are laid out from the right with Name taking the rest, so it is the only edge that can follow the pointer: 8 px, the system's resize cursor, nothing drawn, a double click resets. One set of widths for all of list view, `settings.toml [view.listColumnWidths]`, written once when the drag ends. A narrowing pane squeezes the columns towards their header-sized minimums before it drops any. IPC `shell listColumn <role> <px>|reset`.

## Columns view widths and its path (2026-09-21)

- **A column drags to a width**: the line between two columns is the grip (7 px, the system's resize cursor, nothing drawn; a double click lets the width go). A width belongs to the column's **place** — `c0`, `c1` — not to its folder, as in Finder; the columns not dragged share what is left. `settings.toml [view.columnsWidths]`, written once on release. Forgetting a key is `null` (`Settings.forget`), because the daemon merges map settings and could otherwise never take an entry out of one.
- **The path follows the drilling**: the pane stays on the first column's folder however deep the columns go, so the path over the view — the title bar's, and each pane header's side by side — shows the **deepest open** folder (`ColumnsPane.shownUri`). A pill that is one of the open columns takes the columns back to it (`backTo`): it becomes the last column, nothing in it chosen, the trail keeping its grey. A pill above the first column opens the pane there as before.

## Verification

- `config::view_pref_tests`: set, overwrite (no duplicates), read back, clear.
- Opening a folder after switching it to icon view and sorting by size reopens it that way; a sibling folder is unaffected; "Forget all" returns both to the defaults.
- Switching a column on adds it to the header and every row without a restart, and the rename overlay's width follows.
- A dragged list column and a dragged columns-view column survive a restart, and a double click on either grip forgets the width (`tst_ListPane`, `tst_ColumnsPane`, `view_state.py`).
- Drilling three columns deep moves the path to the deepest folder; clicking the middle pill takes the columns back to it without re-opening the pane (`tst_ColumnsPane`, one through the real shell's title bar).
