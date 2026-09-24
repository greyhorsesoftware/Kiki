# 03 — Search on the rail, and a filter that follows the keyboard in columns (0.1.1)

**Status:** **done 2026-09-24.** Search is the rail's first entry (`Sidebar.qml`, `sidebar-search`, `searchRequested`, `keyOffset` shifted by one); the toolbar's magnifier and the hamburger's "Search everywhere…" row are gone; `Shell.toggleSearch` opens or closes the overlay. In columns the filter goes to `ColumnsPane.focusCache` through `Shell.filterTarget()`/`applyFilter()`, the bar's placeholder is `filterPlaceholder()` ("Filter src"), the keyboard leaving the column clears it (the shell remembers `filteredCache`, since the focus has moved by then), `state()` carries `filterColumn`, and a column whose rows are dealt again re-finds its chosen row by name or closes the columns it opened (`recheckSelection`, on `Reset` — a filter arrives as one, not a splice). Tests: `tst_Sidebar` (new, 5), `tst_ToolbarDisconnect` (hamburger), `tst_ColumnsPane` (3 filter tests), the `side_by_side` and `columns_ops` e2e sections. Two small tweaks; one plan because they are both about where a thing you type goes.

## 1. Search everywhere lives on the rail, above Home

**Now**: search everywhere (`Ctrl+Shift+F`, the overlay over the name index) is a magnifier on
the toolbar's right (`Toolbar.qml:155`); the rail starts with the favorites, Home first.

**Then**: the rail's first entry, above Home, is **Search** — the magnifier — and it opens the
overlay. It is not a favorite (not reorderable, not removable, not in `favorites.toml`); it is
furniture like Trash and the `+`, drawn first. The toolbar's magnifier goes: one place for one
thing, and the rail is where the things you go to live. `Ctrl+Shift+F` is unchanged.

- **Where**: its own line at the top of the rail, before the Favorites section, with the same
  gap under it that separates the sections. In the compact (icons only) rail it is the
  magnifier with the tooltip "Search everywhere · Ctrl+Shift+F"; with labels, "Search".
- **State**: lit (accent) while the overlay is up, the way a favorite is lit while you are in
  it; a click while it is up closes the overlay.
- **Keyboard**: `Ctrl+B` walks the rail, and Search is index 0: `entries` gains a first
  element `{ kind: "search" }`, `keyOffset` adds one for everything after it, `activateKey`
  opens the overlay. `Sidebar` gets a `signal searchRequested()`; the shell wires it to
  `openSearch("")`.
- **Drops**: none. A folder dropped on it is a drop on the rail's top gap, which is "add a
  favorite at the top", as now.
- **Toolbar**: the magnifier button is removed; the toolbar's remaining right-hand cluster is
  info, view, settings. `tst_Toolbar` loses its assertion on the button; the shortcut bar's
  "search" chip stays.

## 2. In columns, the filter filters the column the keyboard is in

**Now**: `/` opens the filter bar and `Pane.setFilter` filters `pane.listing` — which in the
columns view is the **first** column, whatever column the selection is in. Typing `src` with
the keyboard three columns deep filters the root folder and leaves the column you are looking
at alone.

**Then**: the filter applies to the **focused column** (`ColumnsPane.focusCol`, the one with
the keyboard selection): its `WindowCache` is what `filter(text)` is called on. The first
column with the focus behaves exactly as today.

- **The bar**: the same filter bar above the pane, its placeholder naming the column — "Filter
  src" — so it is plain which folder is being narrowed. `Pane.setFilter(text)` asks the view
  for its filter target (`view.filterTarget()`, the focused column's cache in columns, the
  pane's listing elsewhere) rather than always using `listing`.
- **Focus moves**: leaving the filtered column (arrow left or right, a click in another
  column, opening a folder into a new column) **clears** the filter and the bar's text, the
  way opening a folder clears it today. A filter is a thing you type for the column you are
  in; it does not follow you, and it does not linger in a column you have left.
- **The selection under a filter**: rows that no longer match are unselected; if the focused
  row goes, the columns to its right close, as when nothing is selected — a filtered-out folder
  cannot stay open beside its parent.
- **Counts**: the filter bar's "12 of 240" is the focused column's counts (`filterTotal` from
  that cache).
- **IPC and state**: `search(text)` (the harness's filter call) goes to the same target;
  `state().filter` is the focused column's filter text, and `state().filterColumn` says which.

## Tests

- `tst_Sidebar`: Search is the first entry; a click emits `searchRequested`; `Ctrl+B` lands on
  it first and Enter opens the overlay; it is lit while the overlay is up; the favorites'
  `keyed` indexes are one higher than before and still land on the right rows.
- `tst_Toolbar`: no search button; info, view and settings remain in that order.
- `tst_ColumnsPane`: with the keyboard in column 2, `/` + text filters column 2's rows and not
  column 0's; the placeholder names column 2's folder; arrow-left to column 1 clears the filter
  and column 2 shows every row again; a filter that hides the selected folder closes the
  columns to its right; with the focus in column 0 it is the old behaviour to the letter.
- `tests/e2e/flows/columns_ops.py`: one section — three columns deep, filter, the daemon's
  `Filter` request carries the third column's `lid`.

## Size

| | Days |
|---|---|
| Search on the rail, the toolbar button out, tests | ½ |
| The filter's target in columns, tests | ½ |

## Decisions

| | Question | Decided (owner, 2026-09-24) |
|---|---|---|
| D1 | Remove the toolbar's magnifier, or keep both? | **Remove.** One place per thing; the toolbar keeps info, view, settings. |
| D2 | When the focus leaves a filtered column: clear the filter, or keep it on that column? | **Clear.** Same rule as opening a folder today; no column ever shows fewer rows than it has without a bar saying so. |
