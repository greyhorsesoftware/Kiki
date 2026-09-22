# 20 — Settings

**Status:** 8 pages built and tested; Locations, Open in, Plugins not in 0.1.0.

Builds on: `02-shell-and-views.md` (settings file, IPC), and every plan that declares a setting: 06 (locations), 12 (index roots), 13 (editor), 14 (Open in), 15 (git), 16 (project), 17 (devices), 18 (share), 19 (AI).

## Goal

One Settings window for everything kiki can be told, opened from the shortcut bar's `?` menu, the sidebar's gear, or `Ctrl+,`. Every page edits `settings.toml` or the relevant TOML file through the daemon, applies immediately, and shows the current state of anything detected on the machine (editors, tools, plugins, credentials).

## Layout

~~A separate 900×640 window (so it can stay open beside the main one)~~ **Amended 2026-09-21: an overlay over the kiki window** (`ui/SettingsWindow.qml`), 900 px wide at most and the window less a margin otherwise — so it is always above kiki rather than behind it — with a page list on the left and the page on the right, in the same palette and components as the rest of kiki. `Ctrl+,` opens it; `shell settings open|close [page]` drives it. Pages, in order:

| Page | Controls | Backing |
|---|---|---|
| **General** | *(as built)* file icons; default view; sort by; Vim keys; relative dates; the Favorites panel (shown, rail icons grow); show hidden files; remember view per folder; list columns; heat source and the atime caveat (plan 22). ~~folders first; inspector on by default; Forget all; confirm before remote delete; theme: follow Omarchy or pick from the eight themes; font size~~ — the theme is Omarchy's, always, and is never chosen here | `settings.toml [view]`, plan 21 `ViewPrefs` messages |
| ~~**Keys**~~ | **Not a page (2026-09-21).** Rebinding arrived instead of a read-only table: `^?` opens the **rebinding window** (`ui/KeymapWindow.qml`), which edits the one table in `Keymap.qml` and stores the changes under `[keys]` in `settings.toml`. The shortcuts overlay that used to duplicate it is gone | `Keymap.qml`, `settings.toml [keys]` |
| ~~**Locations**~~ | **Not in 0.1.0 (D4).** The sidebar's Locations section is where they are added, edited and removed | `locations.toml`, keyring |
| **Omarchy** | the four integration items (folder handler, Show in folder, Hyprland keys and chooser rule, portal dialogs) with on/off state, per-item Apply/Remove, Make kiki the default, Remove kiki from Omarchy | plan 09 `Integration` / `Integrate` / `Unintegrate` |
| **Search** | index roots (add a folder or a volume), excludes list, index status with size and age, Rebuild | `settings.toml [index]`, plan 12 messages |
| ~~**Open in**~~ | **Not in 0.1.0 (D3, D4).** The list is edited in `open-in.toml` | `open-in.toml`, plan 14 |
| **Share** | installed share plugins: enabled switch and each plugin's form (a secret field travels as a secret). ~~reorder~~ | share configs, plan 18 |
| **Git** | enabled; ignored files: dim, hide, normal; folder aggregation | `settings.toml [git]` |
| **Project mode** | tree width; arrange windows; agent slot on. ~~tree on, strip, hidden~~ (plan 16) | `settings.toml [project]` |
| **AI** | *(was **Jarvis**)* status; provider; custom command; API key to the keyring. The four texts describe a terminal, not a panel (plan 19) | keyring, plan 19 |
| ~~**Devices**~~ | **Moot (D4):** plan 17 is not in 0.1.0, so there are no devices to list | `devices.toml`, plan 17 |
| **About** | version, daemon socket, plugin directory, log location, Reset all settings |

**The eight that exist**, in this order: General, Search, Share, Git, Project mode, AI, Omarchy, About.

Rules: every control writes through immediately with a small "Saved" flash; invalid input is shown inline with the field and never written; a page that depends on a missing tool says so instead of showing dead controls.

## Protocol additions

Existing `Settings` and `SetSettings` cover the TOML pages. Added: ~~`Keymap -> [{ key, action, plan }]` (served from one table so the cheat sheet and the page agree)~~ **struck 2026-09-21: there is no `Keymap` request.** `config.rs` held a third copy of the key table, stale and used by nothing; it went with the shortcuts overlay, and `Keymap.qml` is the one table — the rebinding window edits it, and nothing in the daemon needs to know the keys. `ResetSettings -> {}` and `About -> { version, socket, pluginDir, shareDir, logPath }` are built.

**Two settings written by dragging (2026-09-21):** `[view.listColumnWidths]`, keyed by role (`mtime`, `size`, `kind`) for list view's value columns, and `[view.columnsWidths]`, keyed by a column's **place** (`c0`, `c1`, …) rather than its folder, as in Finder. Both are written once on release of the grip, and a double click lets a width go.

**A map setting can forget a key.** Maps merge — which is what makes a one-key patch possible — so until 2026-09-21 nothing could ever be taken *out* of one: a column reset with a double click was back at the next start. **`null` as a value forgets that key** (`Settings.forget`, `config.rs`). Found beside it: any map-shaped setting made `settings.toml` unreadable, because `toml::write` put a table inside a table out as one line of JSON that `toml::parse` refuses — the file then read as empty, every setting back to its default. Nested tables go out as `[mirror.last]` now.

**IPC added**: `settings(open|close, page?)`.

**Mockup**: a `Settings.dc.html` artboard (General page) and a second state on the Open in page. Add before building.

## Verification

- Changing the default view in Settings writes a **one-key** `SetSettings` patch, changes the next window opened and is present in `settings.toml` (`tst_SettingsWindow`).
- A tree width outside 48–2000 is refused at the field and the file is unchanged.
- ~~The Open in page lists exactly the entries whose `detect` binary is on `PATH`; reordering changes `Alt+Enter`'s default.~~ No such page (D3).
- The AI page stores the key in the keyring only (through `AiConfigure`) and `grep -r` over `~/.config/kiki` finds nothing.
- Reset all settings removes the files and the window shows defaults.
- A list column dragged to a width survives a restart; reset with a double click, it is gone from `settings.toml` and stays gone.
