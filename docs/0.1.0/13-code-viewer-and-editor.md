# 13 — Code viewer and editor bridge

Builds on: `03-inspector.md` (preview, tabs), `02-shell-and-views.md` (settings, IPC, keymap), `09-omarchy-integration.md` (Hyprland hooks).

## Goal

Selecting a code or text file shows it highlighted, with line numbers and a current-line marker, to the right of the listing. Pressing `e` (or Edit) opens it in the editor the user has chosen, placed beside kiki, and keeps a channel open so later selections switch files in that editor and saves reflect back in kiki. No editor is built into kiki.

## Code viewer

The inspector (plan 03) gains a **Code** tab, shown by default instead of General when the file's kind is `code`, `text` or `document` with a text mime type, or when the extension is in the highlight table. General and Permissions remain one click away.

**Highlighting** runs in a helper process, `kiki-helper-highlight`, so tree-sitter and its grammars stay out of kikid. It speaks the plugin framing over a pipe and answers `Highlight { path, lang?, first, count } -> { lines: [[span…]], total }`, where a span is `{ text, class }` and `class` is one of a fixed set (`keyword`, `string`, `comment`, `number`, `type`, `function`, `variable`, `operator`, `punctuation`, `tag`, `attribute`, `heading`, `link`, `emphasis`, `plain`). The daemon maps classes to theme colours from the Omarchy theme and serves the result through the normal window machinery: the viewer is a `WindowCache` over lines, so a 100,000-line file scrolls like a directory does.

- Grammars in 0.1.0: Markdown, C, C++, Rust, Go, Java, Kotlin, TypeScript, JavaScript, Python, Shell, TOML, YAML, JSON, HTML, CSS, SQL, Lua, Nix, QML. Unknown languages render as plain text with line numbers.
- Language detection: extension first, then a shebang, then a modeline in the first three lines.
- Files over 8 MB open in plain mode (no parse); binary files fall back to the General tab.
- Tabs render at the width in `settings.toml` (default 4); trailing whitespace is shown faintly; long lines wrap at the panel width with a hanging indent, toggleable to horizontal scroll.

**Layout**: a gutter with right-aligned line numbers in the muted colour, a 1 px separator, then the text in the mono face. The **current line** (arrow keys or click move it) has a full-width highlight in the surface colour and its number in the foreground colour. Find in file (`/` while the viewer has focus) highlights every match and steps with `n` and `N`. The viewer is read-only; typing does nothing except the keys below.

**Keys** (added to the plan-02 keymap): `e` edit in the chosen editor, `j`/`k` and arrows move the current line, `g g` / `G` top and bottom, `/` find, `n`/`N` next and previous match, `Ctrl+C` copy the selected lines, `Ctrl+L` go to line.

## Editor preference and bridge

Both live in plan 14's Open in list, so there is one mechanism and one settings page for editors, harnesses and other tools. What makes an entry the editor:

- `role = "editor"` on exactly one entry; `e` and the inspector's Edit button run it. First run marks the Neovim preset if `nvim` is on `PATH`, else a `system` entry that runs `xdg-open {file}`.
- `command` launches it (in the chosen terminal for terminal editors), placed beside kiki per plan 14's `placement`.
- `reuse` is the channel: with a session running, later opens and, when `follow = true`, selection changes run this instead of spawning. Neovim uses its listen socket (`{socket}`), VS Code and Zed their reuse-window flags; Helix has no RPC yet, so its entry has no `reuse` and opens a new instance per file.
- `on_save` lets an editor tell kiki about a write in the same frame (Neovim gets a `BufWritePost` autocommand); everything else is caught by the file watch within 100 ms.
- A modified buffer is never replaced: Neovim's `:e` refuses on unsaved changes and kiki shows a toast; VS Code's `--goto` opens a new tab.
- `{line}` carries the code viewer's current line, so `e` lands where the user was reading.
- `qs ipc call kiki reveal {file}` selects a file in the front window; the Neovim launch maps `<leader>k` to it.
- Closing kiki leaves the editor running; the Activity popover or the settings page closes the session.

## Protocol additions

| Request | Fields | Reply |
|---|---|---|
| `OpenText` | `lid`, `uri` | `{ total, lang }`; lines are then served by `Window` on the `lid` as `{ n, spans: [{ text, class }] }` rows |

Editor launch, reuse, sessions and close are plan 14's `OpenIn`, `OpenInSessions` and `OpenInClose` with the `role = "editor"` entry.

**IPC added**: `edit(uri, line?)` (runs the editor entry), `reveal(uri)`, `saved(uri)`.

**Mockups**: a `CodeViewer.dc.html` artboard (inspector Code tab open on a Rust file) is to be drawn before building; an `EditorBeside.dc.html` showing kiki and the terminal editor tiled 50/50 is optional since the terminal's look is Omarchy's, not kiki's.

## Verification

- Every grammar in the list highlights its sample file with no `plain`-only lines; an unknown extension renders plain with numbers.
- A 100,000-line log file opens in the Code tab with the first window painted within a frame and scrolls under 16 ms per frame.
- Find in a 10 MB file reports match count under 200 ms and steps between matches without re-parsing.
- With Neovim as the editor entry: `e` on a file spawns the terminal to the right of kiki at 50/50 at the viewer's current line; selecting a second file switches the buffer through `reuse`; saving in Neovim updates the inspector in the same frame; a modified buffer is not replaced and a toast says so.
- With VS Code as the editor entry: `e` opens or reuses the window at the line; with the `system` entry: `xdg-open` is called once.
- Marking a different entry as editor in Settings ends the old session on the next `e`.
- The `{socket}` path lives under `$XDG_RUNTIME_DIR`, mode 0600, and is removed on `OpenInClose`.
