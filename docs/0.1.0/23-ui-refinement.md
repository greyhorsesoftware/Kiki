# 23 — UI refinement

**Status:** built and tested.

Builds on: `02-shell-and-views.md` (toolbar, keymap), `20-settings.md` (General page), `21-view-memory-and-columns.md` (columns).

## Goal

Small changes that make the shell feel finished: one view button instead of three, hidden files a keystroke away, settings reachable from the toolbar, friendly dates, and a keyboard that can do everything the mouse can.

## View button and menu

- The three-segment view switcher is one 52 px button showing the current view's icon with a chevron. It opens a menu: **Icon** (`Ctrl+1`), **List** (`Ctrl+2`), **Columns** (`Ctrl+3`), ~~**Mirror view** (`Ctrl+4`, plan 24)~~ **Gallery** (`Ctrl+5`, plan 27) with the active one checked, then a separator and **Show hidden files** (`Ctrl+H`) with a check mark when on. **Amended 2026-09-21:** Side by Side is not a view and left this menu (plan 29 J) — it is a toolbar button and `Ctrl+4`, offered only while a server is open or the layout is on (plan 24). The ticks follow the **focused** pane, side by side as well; they used to be withheld there. `viewmenu.js`, `tst_ViewMenu`.
- `ContextMenu` items gained an optional `checked` field: true or false draws a check column, undefined draws none, so the same component serves both menus.

## Hidden files

- A dot-file is hidden by default. `settings view.showHidden` is the starting value for every new pane; `Ctrl+H` or the menu toggles the current pane only, so one pane can show `.git` while the other does not.
- The daemon does the work: `ShowHidden { lid, show }` rebuilds the view like a name filter (the string pool keeps every name, so the toggle is instant and costs no directory read) and sends `Reset`. Hidden entries never enter the window, so a `Ctrl+A` selects only what is shown.

## Settings from the toolbar

- A gear button at the right end of the toolbar opens ~~Settings on the General page, the same as `Ctrl+,`. `?` still opens the Keys page.~~ **Amended 2026-09-21:** a small menu — **Settings…** (`Ctrl+,`, General page), **Keyboard shortcuts…** and **About kiki…**. There is no Keys page in Settings and no shortcuts overlay: `^?` (`Ctrl+?`) opens the **rebinding window** (`KeymapWindow`), which is where the keys are both read and changed, and the bottom bar's hint says `^? keys`. `ShortcutsOverlay.qml` and the daemon's `Keymap` request are gone (plan 31, phase 7): `Keymap.qml` is the one table.

## Relative dates

- **Modified** in the list view shows a friendly form by default: "just now" (under 45 s), "12 min ago", "3 h ago" (same day), "yesterday 14:02", "Tuesday 14:02" (within six days), "12 Sep 14:02" (this year), "12 Sep 2024" (older). The inspector and the search results keep the full date, and the list can too with **Relative dates** off in Settings → General.
- Accessed (plan 22) keeps its own "ago" phrases, which are coarser on purpose because access times are coarse.

## Markdown preview

- A `.md` or `.markdown` file's inspector preview is rendered, not shown as source: Qt's built-in Markdown support (`Text.MarkdownText`) draws headings, emphasis, lists, code spans and blocks, tables, links (opened externally on click) and images by path. The box is taller (300 px) and scrolls.
- The daemon's `Preview` reply carries `markdown: true` for those files and up to 400 lines instead of the usual 40, with `truncated` when the file is longer; the preview appends a one-line note in that case. ~~The code viewer (plan 13) is still the place to read the whole file.~~ **Amended 2026-09-21:** kiki has no built-in code viewer and will not grow one (plan 31, D1); the whole file is read in an editor, through Edit (`F4`) or Open with.

## Keyboard coverage

An audit of every action against the keymap. Added in this plan:

| Key | Action |
|---|---|
| `Up` `Down` | one row; in the icon grid one row of tiles (was one tile) |
| `Left` `Right` | icon view: previous and next tile; columns: pop and push (was Alt only) |
| `Home` `End` | first and last entry |
| `PgUp` `PgDn` | one screen of rows or tiles |
| `Shift` + any of the above | extends the selection |
| `Alt+Up` | parent folder (~~`Backspace` stays "back"~~ — **amended 2026-09-21:** `Backspace` is **parent folder** in the code, `Alt+Left` is back, and the code is right; D19) |
| `Ctrl+H` | show hidden files |
| `Ctrl+F` | filter this folder (alias of `/`; **search everywhere** is `Ctrl+Shift+F`) |
| ~~`Ctrl+Shift+C`~~ `Super+Shift+C` | copy path (**amended 2026-09-21**: the copy/cut/paste family is on `Super`, which Omarchy rewrites to `Ctrl`) |

Fixed: `e` (edit) had become unreachable after `Ctrl+E` (eject) was added to the same key; both now work. A duplicated `F5` case is gone.

**Decided and built since**: type-ahead is the default (typing jumps to the next name starting with the typed prefix, via `SeekName` over the whole view with wrap-around, prefix reset after 800 ms) and the Vim keys `h j k l e` moved behind a **Vim keys** switch on the General page, off by default; `F4` edits in both modes. `Shift+Del` deletes permanently after a confirmation dialog (`ConfirmDialog`), and Empty Trash asks the same way. `Ctrl+B` focuses the sidebar: Up/Down move a highlight across favorites, volumes, locations and devices, Enter opens (or mounts) the row, Esc returns to the pane.

Every context-menu action and toolbar button now has a key except these, left deliberately: **Open with…** (a menu, reached from the context menu on `Menu`/`Shift+F10`), **Compress…** and **Extract** (dialogs), **Empty Trash** (destructive, menu only), and mount and unmount of volumes (sidebar only). ~~and sidebar rows themselves, which have no keyboard focus in 0.1.0~~ — **amended 2026-09-21:** `Ctrl+B` gives the sidebar the keyboard, as the paragraph above says; the two sentences contradicted each other and the code follows the later one.

Both earlier gaps, type-ahead and `Shift+Del`, are closed as described above.

Every chord above can be rebound: `Keymap.qml` is the table, the rebinding window (`^?`) edits it, and a changed chord lives in `settings.toml [keys]`. Contextual keys — the arrows, Enter, Backspace, the Vim keys, type-ahead — are not in it: they mean different things in each view, and there is nothing useful to rebind them to.

## Seen while running it (2026-09-21)

- **A click outside the path field is Escape.** `Ctrl+L` opens the field; it closed only when it lost the keyboard focus, and a click on the files takes focus from nothing, so the field stayed until Escape. A window-wide catcher while the field is up closes it and lets the press through, so the row clicked is still clicked. `tst_BreadcrumbEdit`.
- **Buttons give something back.** `ui/Button.qml` lights under the pointer and, for the length of a press, darkens with the accent on its edge: a button that gives nothing back looks broken.

## Settings

General page: Relative dates, Show hidden files (default for new panes). ~~The keymap table in the daemon (`Keymap`) and plan 02 carry the new rows so the cheat sheet stays complete.~~ **Amended 2026-09-21:** the daemon's copy of the key table and its `Keymap` request are gone — they were read by nothing and had already gone stale. `Keymap.qml` carries the rows, and the rebinding window is the only place they are shown.

## Verification

- `Format.friendlyDate` unit tests for each boundary: 44 s, 59 min, same-day hours, yesterday, six days, this year, last year.
- In the icon view, `Down` moves to the tile directly below at every window width; `PgDn` moves exactly one screen; `End` selects the last tile and scrolls to it.
- `Ctrl+H` shows `.git` in one pane and not the other; `Ctrl+A` after hiding selects only visible rows.
- The gear opens its menu and its first item opens Settings on General; `e` on a file opens the editor; `Ctrl+E` on a device ejects it.
- `Ctrl+L`, then a click on a row: the field closes and that row is the one selected.
