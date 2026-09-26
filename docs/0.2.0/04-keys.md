# 04 — The keys are the Vim keys

**Status:** built, 2026-09-25 (owner: "lets simplify kiki — make defaults for nav the vim ones,
remove preference, and what else can we add"; "hide the shortcuts stuff in bottom bar, make it a
preference, default to off").

## What changed

- **The "Vim keys" preference is gone.** `h j k l` are Left Down Up Right in every view; `e`
  edits, `i` toggles the info panel. They were a switch under Settings › General, off by
  default; the switch, its two catalog keys and `view.vimKeys` in settings.toml are removed (an
  old file's value is ignored).
- **Type-ahead is gone with it.** Bare letters are commands now, so typing a name to jump to it
  cannot stay; `/` (and `f`) filter, which is the same thing said better. `SeekName` stays in the
  daemon for the code that seeks by name.
- **The rest of a Vim-style file manager's letters**, added from the list the owner brought:

  | key | does |
  |---|---|
  | `j` `k` | down, up (with Shift: extend) |
  | `h` `l` | up a folder, into the folder (columns: left and right a column) |
  | `v` | extend from here: `j k`, the arrows, Home and End grow the selection until Esc, a folder change, or the selection is used |
  | `y` `x` `p` | copy, cut, paste (beside `Super+C/X/V`) |
  | `r` | rename (beside `F2`) |
  | `dd` | trash (beside `Del`); the first `d` waits 600 ms for the second |
  | `z` / `Z` | undo / redo (beside `Ctrl+Z` / `Ctrl+Shift+Z`) |
  | `f` | filter (beside `/`); in the gallery `f` stays the filmstrip |
  | `:` | type a path (beside `Ctrl+L`) |
  | `m` | the context menu (beside the Menu key) |
  | `.` | hidden files — the only binding; `Ctrl+H` is gone from the table (owner: ". should be the only show hidden item") |
  | `Ctrl+T` | a terminal here — new in the shortcuts table, rebindable |

  Not taken from the list: Quick Look on Space and tabs (`t` `w` `1–9`) — features kiki does not
  have, not bindings. The gallery keeps its own bare keys (`0 1 + - f Space`).
- **The bottom bar's shortcut chips are a preference, off by default** (`view.shortcutChips`,
  Settings › General "Shortcuts in the bottom bar"). Messages still roll into the bar.

## Where

`Shell.qml` — `vimKey()`, `keyDown/Up/Left/Right()` shared by the arrows and the letters,
`visual`, `pendingD`; `Keymap.qml` — `terminal` in, `hidden` out; `KeymapWindow.qml` — the list takes what the intro leaves (it ran out of the panel once the intro wrapped); `SettingsWindow.qml`; `kikid/src/config.rs`
(the default); `tests/e2e/flows/vim_keys.py` drives every letter through the real window.
