# 21 — View memory per folder and list columns

Builds on: `01-daemon-and-listing.md` (`Meta`, sort roles), `02-shell-and-views.md` (panes, list view), `20-settings.md` (General page).

## Goal

A folder opens the way you last looked at it, and the list view shows the columns you chose. The Accessed column that plan 22 adds is one of those columns.

## View memory per folder

- **What is remembered**: view mode (icon, list, columns), sort role and sort order, per exact folder. Nothing is inherited by subfolders: a photo folder's icon view should not leak into its `raw` subfolder.
- **Keying**: by URI with the trailing slash stripped, so local, remote (`sftp://name/path`) and `trash:///` listings all work the same way.
- **Storage**: `~/.config/kiki/views.toml`, `[[folder]] uri, view, sort, order, at`. Capped at the most recent 1,000 folders by `at` (the last time the entry was set); older entries fall off.
- **Behaviour**: `Pane.open` looks the folder up and applies the entry, else keeps the current view and the global default sort. Changing the view (toolbar, `Ctrl+1/2/3`) or the sort (header click) writes the entry immediately. A second window picks changes up from the `ViewPrefsChanged` event.
- **Settings → General**: "Remember view per folder" (on by default) and "Forget all". With the switch off, nothing is read or written and every folder uses the defaults.
- **Protocol**: `ViewPrefs -> { folders: { uri: { view, sort, order } } }`, `SetViewPref { uri, view, sort, order }`, `ClearViewPrefs`, event `ViewPrefsChanged { uri? }`.

## List columns

- Name is always first. **Modified**, **Size**, **Kind** and **Accessed** are each optional; the set lives in `settings.toml [view] columns` (default `["mtime", "size", "kind"]`) and is edited on the General page with one switch per column. The list header and rows are built from that list, so column widths and the rename overlay follow it.
- **Accessed** is the access heat map column; its rendering, data source and the `relatime` caveat are plan 22.

## Verification

- `config::view_pref_tests`: set, overwrite (no duplicates), read back, clear.
- Opening a folder after switching it to icon view and sorting by size reopens it that way; a sibling folder is unaffected; "Forget all" returns both to the defaults.
- Switching a column on adds it to the header and every row without a restart, and the rename overlay's width follows.
