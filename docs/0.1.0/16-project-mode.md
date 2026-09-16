# 16 — Project mode

Builds on: `02-shell-and-views.md` (views, IPC), `14-open-in.md` (entries, sessions, placement), `13-code-viewer-and-editor.md` (editor entry), `15-git-status.md` (badges), `09-omarchy-integration.md` (Hyprland).

## Goal

Select a folder, press `e` (or Edit), and the screen becomes `kiki | editor | agent`: kiki collapses to a narrow file tree rooted at that folder, the editor entry opens beside it with the folder as its workspace, and the agent entry opens beside the editor in the same folder. Clicking a file in the tree opens it in the editor; the editor and the agent keep running when kiki leaves project mode. Nothing is embedded; Hyprland does the tiling and kiki drives it.

## Layout

```
┌────────┬──────────────────────────┬────────────────────┐
│ kiki   │ editor                   │ agent              │
│ tree   │ (Neovim, VS Code, …)     │ (Claude Code, …)   │
│ 320 px │ remaining width, ~55 %   │ ~30 %              │
└────────┴──────────────────────────┴────────────────────┘
```

- **kiki in project mode**: sidebar and toolbar hide; the window shows a header with the project name, the branch chip and a Leave button, the **tree view** below, a search box that filters the tree, and the shortcut bar. Target width 320 px (`[project] width` in settings). If the editor entry is a GUI with its own tree, such as VS Code, the user can set `[project] tree = false` and kiki narrows to a 48 px strip with the branch chip and Leave button only, or hides entirely (`tree = "hide"`) and comes back with `Ctrl+Shift+P`.
- **Editor slot**: the plan-14 entry marked `role = "editor"`, launched with `{dir}` = the project root; `follow = true` entries receive each file clicked in the tree through `reuse`.
- **Agent slot**: the entry marked `role = "agent"` (new in plan 14; Claude Code by default when detected), launched in the project root. Optional: `[project] agent = false` gives a two-window layout.
- **Arrangement**: kikid applies it through Hyprland once each window exists: it reads `hyprctl clients -j`, finds the three windows by class and pid, then dispatches `movewindow` and `resizeactive` so kiki sits at the left with a fixed width and the editor and agent split the rest at the configured ratio. Windows that already exist (an editor session from earlier) are moved, not respawned. Arrangement is best effort: if a window cannot be found within 5 s it is left where Hyprland put it and a toast says so. Users who prefer their own layout set `[project] arrange = false`.
- **Leaving**: `e` on the project header, Esc with the tree focused, or `Ctrl+Shift+P` restores kiki's normal chrome and previous width and returns the pane to the project root as an ordinary listing. The editor and agent are untouched. Closing kiki leaves them too.
- **Re-entering** the same folder later reuses running editor and agent sessions and only rearranges.

## Tree view

A fourth view mode, available only in project mode in 0.1.0 (a general tree view is a later option).

- Rows are the project's directories and files with indentation, disclosure chevrons, kind icons or thumbnails, and plan-15 git badges; ignored entries dimmed, `.git` hidden.
- The tree is **virtualised** like everything else: the daemon keeps a flattened list of visible rows (expanded state per directory) and serves it through the window protocol, so a project with 100,000 files scrolls at 60 fps and only expanded directories are listed. Expanding a directory opens its listing (from the cache when watched), inserts its rows, and sends `Reset`.
- Expanded state is remembered per project root in `~/.local/state/kiki/projects.toml`, along with the last selected file.
- Keys: `Enter` or click opens the file in the editor (`reuse`) and keeps focus in the tree; `Space` toggles a directory; `h`/`l` collapse and expand; `/` filters the tree by name (matching files stay visible with their ancestors expanded); `Alt+Enter` sends the selection to the agent entry as `{files}`; `r` reveals the editor's current file in the tree when the editor reports it (Neovim's `<leader>k` mapping from plan 13); `n` new file, `N` new folder, `F2` rename, `Del` trash, all as plan-04 jobs.
- The inspector is not shown in project mode; the editor is the inspector.

## Protocol additions

| Request | Fields | Reply |
|---|---|---|
| `OpenTree` | `lid`, `uri` | `{ n }`; rows via `Window` on the `lid` as `Row` plus `{ depth: u8, expanded: bool \| null, rel: string }` |
| `TreeExpand` | `lid`, `first` (row index), `expanded: bool` | `{ n }` then `Reset` |
| `TreeFilter` | `lid`, `text` | `{ n }` then `Reset` |
| `TreeReveal` | `lid`, `uri` | `{ row: u32 }` (expands ancestors as needed, then `Reset`) |
| `Arrange` | `layout: "project"`, `root: Uri`, `windows: [{ role, class, pid }]` | `{ arranged: [role], missing: [role] }` |

`OpenIn` gains `role` as an alternative to `id`. Plan 14's entries gain `role = "agent"`.

**IPC added**: `project(enter|leave, uri?)`, `projectState()` (root, tree width, arranged roles).

**Mockup**: `ProjectMode.dc.html` in `docs/design/`, a 2200 px wide artboard with the three windows tiled: kiki's 320 px tree (header with project name, branch chip and Leave; filter; tree with `docs/` and `src/` expanded, git badges, `main.rs` selected; shortcut bar), a Neovim placeholder in the middle with focus, and a Claude Code placeholder at the right. The editor and agent frames only indicate proportions; their look is the tools' own.

## Verification

- With Neovim as editor and Claude Code as agent on `PATH`: `e` on a folder narrows kiki to 320 px at the left, spawns both in the folder, and arranges them left to right within 2 s; `hyprctl clients -j` shows the three windows in that order.
- Clicking five files in the tree in succession switches the Neovim buffer each time with no new window; the editor's cursor lands on line 1.
- Esc restores kiki's previous width and chrome; the editor and agent are still running; re-entering rearranges without spawning (pid unchanged).
- A 100,000-file project: `OpenTree` paints within a frame; expanding a 5,000-entry directory inserts rows under 50 ms; scrolling stays under 16 ms per frame.
- Filtering the tree by a name shows matches with ancestors expanded and nothing else; clearing restores the remembered expansion.
- Git badges in the tree update within 400 ms of a save in the editor.
- With `arrange = false` nothing is moved; with the agent entry absent, the layout is two windows and no error.
