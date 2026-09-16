# 14 — Open in… (editors, agent harnesses and external tools)

Builds on: `02-shell-and-views.md` (toolbar, context menu, settings, IPC), `13-code-viewer-and-editor.md` (terminal spawning and Hyprland placement), `09-omarchy-integration.md`.

## Goal

Select a folder or files, press one button, and the selection opens in an editor (Neovim, Helix, VS Code, Zed), an AI coding harness (Claude Code, Codex, Gemini CLI, Aider, OpenCode, Hermes, Muse, anything else) or any other external tool. kiki knows nothing about the tools themselves: each is a command template with placeholders, run in a terminal or directly, placed beside kiki. Adding a new one is a few lines of TOML, no code. One list serves both `e` (the entry marked as the editor) and `Alt+Enter` (the default tool); plan 13's editor bridge is this mechanism plus a `reuse` channel.

## Configuration

`~/.config/kiki/open-in.toml` holds one `[[tool]]` per entry. A shipped defaults file under `/usr/share/kiki/open-in.toml` provides presets; the user's file adds to or overrides them by `id`.

```toml
[[tool]]
id = "claude"
name = "Claude Code"
detect = "claude"                 # binary on PATH; entry is hidden when missing
role = "agent"                    # the project-mode agent slot (plan 16); one entry may carry this
command = "claude"                # run with cwd = {dir}
terminal = true                   # run inside the chosen terminal (plan 13's [editor].terminal)
accepts = "both"                  # folder | file | both
placement = "right"               # right | left | float | none | tab (a new tab in the running terminal when supported)
prompt = "Look at {files}"        # optional: appended as the first argument when files are selected
icon = "sparkle"                  # from kiki's icon set, or a freedesktop icon name

[[tool]]
id = "codex"
name = "Codex"
detect = "codex"
command = "codex"
terminal = true
accepts = "both"

[[tool]]
id = "neovim"
name = "Neovim"
detect = "nvim"
role = "editor"                   # the entry `e` uses on a file; exactly one entry may carry this (`role = "agent"` marks the project-mode agent slot, plan 16)
command = "nvim --listen {socket} {file}"
reuse = "nvim --server {socket} --remote-send ':e {file}<CR>:{line}<CR>'"
on_save = "qs ipc call kiki saved {file}"   # injected into the editor when it supports a hook (Neovim: BufWritePost)
terminal = true
accepts = "file"
follow = true                     # later selections are sent through `reuse` while the session runs

[[tool]]
id = "vscode"
name = "VS Code"
detect = "code"
role = "editor"
command = "code --new-window {dir}"
reuse = "code --reuse-window --goto {file}:{line}"
terminal = false
accepts = "both"
follow = true

[[tool]]
id = "gemini"
name = "Gemini CLI"
detect = "gemini"
command = "gemini"
terminal = true
accepts = "both"

[[tool]]
id = "aider"
name = "Aider"
detect = "aider"
command = "aider {files}"
terminal = true
accepts = "both"

[[tool]]
id = "opencode"
name = "OpenCode"
detect = "opencode"
command = "opencode"
terminal = true
accepts = "both"

[[tool]]
id = "terminal"
name = "Terminal here"
command = "$SHELL"
terminal = true
accepts = "folder"
placement = "right"

[[tool]]
id = "lazygit"
name = "lazygit"
detect = "lazygit"
command = "lazygit"
terminal = true
accepts = "folder"
```

Hermes, Muse and any harness kiki does not ship a preset for are added the same way; the user copies a block, sets `detect`, `command` and the placeholders that harness takes. Presets are only a convenience; nothing in kiki depends on knowing a harness's flags.

**Placeholders**, substituted before the command is run through `sh -c` with proper quoting:

| Placeholder | Value |
|---|---|
| `{dir}` | the selected folder, or the parent of the selected file; also the working directory |
| `{file}` | the single selected file (entry is disabled for multi-selection when only `{file}` is used) |
| `{files}` | all selected files, quoted, space-separated |
| `{uri}` / `{uris}` | the same as URIs (for tools that understand `sftp://`) |
| `{prompt}` | the entry's `prompt` template after substitution, so a harness that takes an initial message gets one |
| `{name}` | the location name for remote selections |
| `{line}` | the line to open at (the code viewer's current line; 1 otherwise) |
| `{socket}` | a per-entry Unix socket path under `$XDG_RUNTIME_DIR`, mode 0600, for tools that expose an RPC on one |

**Sessions and `reuse`**: an entry with a `reuse` template gets a session. The first launch runs `command`; while that process lives, later launches run `reuse` instead (no new window), and if `follow = true` kiki runs it automatically when the selection changes to a file the entry accepts. A tool without `reuse` is launched fresh each time. `on_save` is a command the tool runs after writing a file, injected for editors kiki knows how to hook (Neovim through an autocommand); every other tool's saves are caught by the file watch within 100 ms. Sessions belong to the daemon and survive kiki windows; the Activity popover lists them with a close action.

**Remote selections**: a tool runs on this machine, so for a remote folder kiki offers to open the location's `local_uri` counterpart when one is configured, or passes `{uris}` if the entry declares `remote = true`. Otherwise the entry is disabled with a tooltip.

**Environment**: the spawned process gets the user's environment plus `KIKI_SELECTION` (newline-separated paths) and `KIKI_DIR`, so a harness's own hooks can read the selection without placeholders.

## UI

- **Toolbar button** "Open in" with a dropdown of enabled tools, the default first. Clicking the button itself runs the default; the arrow opens the list. The default is the first entry in the user's file, or the most recently used.
- **Context menu**: an "Open in ▸" submenu with the same list, on folders and files.
- **Keys**: `e` runs the `role = "editor"` entry on a selected file (plan 13) and enters project mode on a selected folder (plan 16); `Alt+Enter` runs the default tool on the selection; `Alt+Shift+Enter` opens the dropdown. All added to the plan-02 keymap and the cheat sheet.
- **Placement** as in plan 13: Hyprland tiles the spawned terminal beside kiki. Each launch is a new terminal window unless `placement = "tab"` and the terminal supports opening a tab in its running instance (Ghostty, kitty and foot do through their IPC; the daemon uses it when available and falls back to a window).
- **Settings page** "Open in": the merged list with detected state, drag to reorder (the first is the default), a radio marking which entry is the editor, edit fields inline, add from a preset picker or blank, a terminal picker (`terminal = "auto"` means Omarchy's default), and a "Test" button that shows the substituted command for the current selection.
- **Shortcut bar** shows `Alt+Enter open in <default name>` whenever the selection allows it.

## Daemon

The daemon does the spawning so a launched tool survives the kiki window and so placement is applied consistently with the editor bridge.

| Request | Fields | Reply |
|---|---|---|
| `OpenInList` | | `{ tools: [{ id, name, icon, accepts, enabled, reason? }] }` (detection resolved) |
| `OpenIn` | `id`, `uris: [Uri]`, `line?` | `{ pid, reused: bool }` or error `Invalid` with the placeholder that could not be filled |
| `OpenInSessions` | | `{ sessions: [{ id, pid, files: [Uri] }] }` |
| `OpenInClose` | `id` | `{}` (graceful: the entry's `quit` command if set, else `SIGTERM`) |
| `OpenInTest` | `id`, `uris` | same as `OpenIn` but returns the fully substituted command without running it |

Events: `OpenInChanged {}` when either TOML file changes (the daemon watches both).

**IPC added**: `openIn(id?)`, `openInList()`.

**Mockups**: add the "Open in" dropdown (toolbar, open, listing Claude Code, Codex, Aider, Terminal here, lazygit with the default marked) to the toolbar artboards, and the "Open in ▸" submenu to the context menu artboard, before building.

## Verification

- With `claude` on `PATH` and a folder selected, `Alt+Enter` spawns the chosen terminal to the right of kiki running Claude Code with the working directory set to the folder; with three files selected, `KIKI_SELECTION` lists them and `{files}` expands quoted in the command.
- An entry whose `detect` binary is absent does not appear in the dropdown; adding the binary to `PATH` and touching the TOML file makes it appear without a restart.
- A user entry with the same `id` as a preset overrides it; a malformed entry is reported in the Settings page with the line number and the rest still load.
- `OpenInTest` shows the exact command for an entry using every placeholder, correctly quoted for names with spaces and quotes.
- A remote selection on a location with a `local_uri` opens the local counterpart; one without is disabled with the reason shown.
- `placement = "tab"` opens a tab in a running Ghostty and falls back to a new window in a terminal without IPC.
