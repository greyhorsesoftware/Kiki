# 20 — Settings

Builds on: `02-shell-and-views.md` (settings file, IPC), and every plan that declares a setting: 06 (locations), 12 (index roots), 13 (editor), 14 (Open in), 15 (git), 16 (project), 17 (devices), 18 (share), 19 (AI).

## Goal

One Settings window for everything kiki can be told, opened from the shortcut bar's `?` menu, the sidebar's gear, or `Ctrl+,`. Every page edits `settings.toml` or the relevant TOML file through the daemon, applies immediately, and shows the current state of anything detected on the machine (editors, tools, plugins, credentials).

## Layout

A separate 900×640 window (so it can stay open beside the main one) with a page list on the left and the page on the right, in the same palette and components as the rest of kiki. Pages, in order:

| Page | Controls | Backing |
|---|---|---|
| **General** | default view; sort and order; folders first; show hidden files; inspector on by default; confirm before remote delete; theme: follow Omarchy or pick from the eight themes; font size | `settings.toml [view]`, `[theme]` |
| **Keys** | the keymap as a read-only table with a search box; a note that rebinding is a later version | plan 02's table |
| **Locations** | the saved locations with edit and remove; Add opens plan 06's dialog | `locations.toml`, keyring |
| **Search** | index roots (add a folder or a volume), excludes list, index status with size and age, Rebuild | `settings.toml [index]`, plan 12 messages |
| **Open in** | the merged tool list: detected state, drag to reorder, editor radio, agent radio, inline field edit, add from preset, Test | `open-in.toml`, plan 14 |
| **Share** | installed share plugins: enabled switch, each plugin's form, reorder | share configs, plan 18 |
| **Git** | enabled; ignored files: dim, hide, normal; folder aggregation | `settings.toml [git]` |
| **Project mode** | tree width; tree on, strip, hidden; arrange windows; agent slot on | `settings.toml [project]` |
| **Jarvis** | provider; API key (stored to keyring, shown masked with Clear); model; active credential source; a Test button; link to pricing | keyring, plan 19 |
| **Devices** | detected devices and their kind; rename; forget a remembered device | `devices.toml`, plan 17 |
| **About** | version, daemon socket, plugin directory, log location, Reset all settings |

Rules: every control writes through immediately with a small "Saved" flash; invalid input is shown inline with the field and never written; a page that depends on a missing tool says so instead of showing dead controls.

## Protocol additions

Existing `Settings` and `SetSettings` cover the TOML pages. Added: `Keymap -> [{ key, action, plan }]` (served from one table so the cheat sheet and the page agree), `ResetSettings -> {}`, `About -> { version, socket, pluginDir, shareDir, logPath }`.

**IPC added**: `settings(open|close, page?)`.

**Mockup**: a `Settings.dc.html` artboard (General page) and a second state on the Open in page. Add before building.

## Verification

- Changing the default view in Settings changes the next window opened and is present in `settings.toml`.
- A malformed value typed into the tree width is shown inline and the file is unchanged.
- The Open in page lists exactly the entries whose `detect` binary is on `PATH`; reordering changes `Alt+Enter`'s default.
- The AI page stores the key in the keyring only (`secret-tool lookup app kiki service anthropic`) and `grep -r` over `~/.config/kiki` finds nothing.
- Reset all settings removes the files and the window shows defaults.
