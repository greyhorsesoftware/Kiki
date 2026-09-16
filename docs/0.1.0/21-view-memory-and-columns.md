# 21 — View memory per folder and list columns

Builds on: `01-daemon-and-listing.md` (`Meta`, sort roles), `02-shell-and-views.md` (panes, list view), `20-settings.md` (General page).

## Goal

A folder opens the way you last looked at it, and the list view shows the columns you chose, including an **Accessed** column that makes recently touched files visually obvious.

## View memory per folder

- **What is remembered**: view mode (icon, list, columns), sort role and sort order, per exact folder. Nothing is inherited by subfolders: a photo folder's icon view should not leak into its `raw` subfolder.
- **Keying**: by URI with the trailing slash stripped, so local, remote (`sftp://name/path`) and `trash:///` listings all work the same way.
- **Storage**: `~/.config/kiki/views.toml`, `[[folder]] uri, view, sort, order, at`. Capped at the most recent 1,000 folders by `at` (the last time the entry was set); older entries fall off.
- **Behaviour**: `Pane.open` looks the folder up and applies the entry, else keeps the current view and the global default sort. Changing the view (toolbar, `Ctrl+1/2/3`) or the sort (header click) writes the entry immediately. A second window picks changes up from the `ViewPrefsChanged` event.
- **Settings → General**: "Remember view per folder" (on by default) and "Forget all". With the switch off, nothing is read or written and every folder uses the defaults.
- **Protocol**: `ViewPrefs -> { folders: { uri: { view, sort, order } } }`, `SetViewPref { uri, view, sort, order }`, `ClearViewPrefs`, event `ViewPrefsChanged { uri? }`.

## List columns

- Name is always first. **Modified**, **Size**, **Kind** and **Accessed** are each optional; the set lives in `settings.toml [view] columns` (default `["mtime", "size", "kind"]`) and is edited on the General page with one switch per column. The list header and rows are built from that list, so column widths and the rename overlay follow it.
- **Accessed** shows the last access time as a relative phrase ("just now", "5 min ago", "3 h ago", "2 days ago", "3 weeks ago", "5 months ago", "2 years ago") over a heat swatch in the accent colour. The swatch is fully saturated for the last hour and fades on a log scale to nothing at about a year, so a folder scanned at a glance shows what was touched recently. The column sorts (`Sort { role: "atime" }`).
- **Where the time comes from**: `Meta.atime` (ms since the epoch, 0 = unknown). Locally it comes from the same `statx` call as size and mtime, with `STATX_ATIME` added to the mask, so it costs nothing extra. Remote plugins that do not report access times show a dash.
- **Caveat, stated in the UI**: Linux's default `relatime` mount option updates atime at most once a day unless the file changed since it was last read, so on a stock Omarchy install the column is accurate to the day. `strictatime` in the mount options gives minute accuracy. A future variant can log kiki's own opens and prefer that over `atime`.

## Verification

- `config::view_pref_tests`: set, overwrite (no duplicates), read back, clear.
- Opening a folder after switching it to icon view and sorting by size reopens it that way; a sibling folder is unaffected; "Forget all" returns both to the defaults.
- Switching Accessed on adds the column to the header and every row without a restart; `atime` sorts; files read a moment ago show a bright swatch and files untouched for months show none.
- On a `relatime` mount, reading a file twice within a day changes the column once.
