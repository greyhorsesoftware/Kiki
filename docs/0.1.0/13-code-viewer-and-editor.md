# 13 — Editor bridge

**Status:** no built-in viewer, by decision; nvim bridge built, hand-verified.

Builds on: `02-shell-and-views.md` (settings, IPC, keymap), `14-open-in.md` (the tool list, sessions, placement), `09-omarchy-integration.md` (Hyprland hooks).

**The code viewer is deleted, and this plan is renamed for what is left (D1, owner 2026-09-19): kiki has no built-in code viewer and will not grow one** — code is read in an editor, which is what Open in and this bridge are for, and a second place to read code is a second thing to keep right. Gone with that decision: the inspector's **Code tab** (`CodeTab.qml`, commit 800883b), `kiki-plugin-highlight` with its sixteen tree-sitter grammars, the `OpenText` and `TextFind` requests, `helpers::highlight`, `editor.tabWidth`, the viewer's keys and layout, and the Verification bullets that measured them. The inspector's text and Markdown preview (`preview.rs`) is a different thing and stays. This is a decision about what kiki is, not a deferral.

## Goal

`F4` (Edit) on a file opens it in the editor the user has chosen, in a terminal placed beside kiki, and keeps a channel open so the next `F4` switches that editor's buffer rather than opening a second window. Saves come back to kiki. No editor is built into kiki.

## Editor preference and bridge

The entries live in plan 14's Open in list, so there is one mechanism for editors, harnesses and other tools. What makes an entry the editor:

- `role = "editor"` on exactly one entry; `F4` (and `e` with Vim keys) and the inspector's Edit button run it. The shipped preset carrying it is Neovim.
- `command` launches it, in the chosen terminal for terminal editors, placed beside kiki per plan 14's `placement`.
- `reuse` is the channel: with a session running, a later Edit runs this instead of spawning. Neovim uses its listen socket (`{socket}`); VS Code and Zed have `reuse` templates but no `role`, so nothing runs them as the editor today. Helix has no RPC, so its entry has no `reuse` and opens a new instance per file.
- `on_save` lets the editor tell kiki about a write. **As built it is not a separate field**: the Neovim preset carries its own `--cmd 'autocmd BufWritePost * silent! !kiki --ipc saved %:p'`, which calls back through the launcher. Every other tool's saves are caught by the file watch within 100 ms.
- A modified buffer is never replaced: the `reuse` template's `:e` refuses on unsaved changes.
- `{line}` is what the caller asks for and 1 otherwise — it used to carry the code viewer's current line, and there is no viewer now.
- `kiki --ipc reveal <file>` selects a file in the front window; the Neovim launch maps `<leader>k` to it. (Not `qs ipc call kiki …`: that named config is not installed, so those calls went nowhere until the launcher took them.)
- Closing kiki leaves the editor running; the session is closed from `OpenInClose`.

**The socket** (`{socket}`, `nvim --listen`): under `$XDG_RUNTIME_DIR`, in a directory of kiki's own that is made 0700 and refused unless it is a real directory, ours, and closed to others — a symlink or an open directory under that name is refused. A finished session's socket file is cleared, so the next `--listen` does not fail with "address in use". *Fixed in phase 2 (2026-09-19)*: with no runtime directory the socket used to go into plain `/tmp`, where any local user could connect and drive the editor as you.

## Gaps, all post-0.1.0

- **`follow` is stored and never read.** It is set on the presets and written to `open-in.toml`, but `OpenInList` does not even report it and nothing in the shell watches the selection: a later selection never reaches a running editor. Only an explicit Edit does.
- **No fallback when `nvim` is absent.** `role = "editor"` is found by role without regard to detection, so on a machine without Neovim `F4` answers *"Neovim is not installed"* rather than falling back to the `system` entry (`xdg-open {file}`), which is sitting right there in the list.
- Marking a *different* entry as the editor means editing `open-in.toml` by hand: there is no Settings page for it (D3, plan 14).

## Protocol

~~`OpenText { lid, uri }`~~ — deleted with the viewer (D1); `TextFind` with it. Editor launch, reuse, sessions and close are plan 14's `OpenIn`, `OpenInSessions` and `OpenInClose`, which take **`tool`** or `role` (the field was `id`, which collided with the request's own number — fixed 2026-09-21).

**IPC**: `edit(uri, line?)` runs the editor entry; `reveal(uri)` selects a file in the front window (the project tree when project mode is up); `saved(uri)` refreshes the listing.

~~**Mockups**: `CodeViewer.dc.html`, `EditorBeside.dc.html`.~~ Neither is needed: there is no viewer, and the terminal's look is Omarchy's.

## Verification

- With Neovim as the editor entry: `F4` on a file spawns the terminal beside kiki at the file; a second `F4` on another file switches the buffer through `reuse` with no new window; saving in Neovim updates the listing; `<leader>k` reveals the file in kiki.
- With no `nvim` on `PATH`, `F4` says so in a toast and starts nothing.
- The `{socket}` path lives under `$XDG_RUNTIME_DIR` in a directory that is ours and 0700, and is cleared when the session ends.
- Nothing in the tree or the package answers to `OpenText`, `TextFind` or `kiki-plugin-highlight`, and no plan promises a code viewer.
