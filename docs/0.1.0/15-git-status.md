# 15 — Git status

Builds on: `01-daemon-and-listing.md` (windows, watch, pushed rows), `02-shell-and-views.md` (views, breadcrumb), `03-inspector.md` (Git field).

## Goal

Files and folders inside a git repository show their status in every view, the breadcrumb shows the branch, and the inspector shows the file's git detail, without slowing the listing down or adding a git library to the daemon.

## Design

**Source**: the `git` binary, spawned by kikid. No libgit2, no gitoxide; both are large dependency graphs and `git` is always present on Omarchy. Two commands are used:

- `git -C <dir> status --porcelain=v2 -z --branch --untracked-files=all --ignored=matching -- .` for the state of everything under the listed directory. Porcelain v2 with `-z` is a stable machine format: `1` ordinary entries with staged and unstaged codes, `2` renames, `u` conflicts, `?` untracked, `!` ignored, plus `# branch.*` header lines carrying branch, upstream, ahead and behind.
- `git -C <root> log -1 --format=%H%x00%h%x00%an%x00%at%x00%s -- <file>` for the inspector's last-commit line, only when the Git field is visible.

**Repository detection**: for a listed directory, walk up to find `.git` (a directory, or a file for worktrees and submodules). Cache the root per directory path; a negative result is cached too, so directories outside any repository cost one check. Remote locations never run git.

**Scheduling**: status runs on the worker pool at low priority after phase 1 has painted, once per listing directory, and its result is cached per (root, directory). Each entry in the listing is mapped to a state; folders aggregate their subtree: modified if any descendant is modified, added or deleted; untracked if every descendant is untracked; ignored if the folder itself is ignored; conflicted if any descendant is. States are pushed into live windows as `Rows` with a `git` field, exactly like thumbnails, so the listing never waits on git.

**Invalidation**: the existing watch. Any change under the listed directory, or to `<root>/.git/index`, `HEAD`, `MERGE_HEAD` or `refs/`, re-runs status for the affected directories after a 300 ms debounce. `.git` itself is never listed with status.

**Big repositories**: the pathspec keeps a status run proportional to the subtree in view. On repositories over 50,000 tracked files the daemon suggests, once, enabling `core.untrackedCache` and `core.fsmonitor` in a toast with a one-click `git config`. If a status run exceeds 2 s the directory is marked slow and re-run only on explicit refresh.

**Branch chip**: `.git/HEAD` is read directly (a symbolic ref or a detached hash), so the chip costs one small file read per navigation and no process. Ahead and behind come from the status header when available.

## UI

| Surface | Shows |
|---|---|
| List and columns rows | a one-letter badge after the name: `M` modified (yellow), `A` added (green), `D` deleted (red), `R` renamed (yellow), `!` conflicted (red), `?` untracked (muted green); ignored files and folders dimmed |
| Icon tiles | a coloured dot at the top-right of the icon in the same colours; ignored dimmed |
| Folders | the aggregated state as a dot; a repository root folder additionally shows the branch name under its name in list view |
| Breadcrumb | a chip `⎇ main ↑2 ↓1` at the right end of the path when the pane is inside a repository; click copies the branch name |
| Inspector, General tab | Git: state, branch, last commit (short hash, author, relative date, subject) |
| Sidebar Favorites | a small dot on a favorite that is a dirty repository root |

Settings (`[git]` in `settings.toml`): `enabled = true`, `show_ignored = "dim" | "hide" | "normal"`, `folders = "aggregate" | "off"`.

Not in 0.1.0: staging, unstaging, discarding, commit, diff view, submodule recursion, status on remote locations. Context-menu git actions are a later plan.

## Protocol additions

`Row` gains `git: { state: "modified" | "added" | "deleted" | "renamed" | "conflicted" | "untracked" | "ignored" | "clean", staged: bool } | null` (`null` outside a repository or before status has run).

| Request | Fields | Reply |
|---|---|---|
| `Repo` | `uri` | `{ root: Uri, branch: string \| null, detached: bool, ahead: u32, behind: u32, dirty: bool } \| null` |
| `GitStatus` | `uri` | `{ state, staged, branch, last: { hash, short, author, time, subject } \| null }` (replaces plan 03's shape) |
| `GitRefresh` | `uri` | `{}` (re-run status for that directory now) |

Event: `RepoChanged { root }` when HEAD or the index changes, so the breadcrumb chip updates.

**IPC added**: `gitState(name)` for tests.

**Mockups**: add badges to `ListView.dc.html` rows and dots to `IconView.dc.html` tiles, plus the breadcrumb chip on `Main.dc.html`, before building.

## Verification

- In a test repository with one file of each state (modified, staged, deleted, renamed, conflicted, untracked, ignored), every row shows the right badge and colour, and the folder containing them shows the aggregated dot.
- The listing paints before status runs; status for a 10,000-file subtree lands within 500 ms warm and is pushed only to rows in the live window.
- Editing a tracked file updates its badge within 400 ms without re-listing; `git commit` updates the branch chip and clears badges within the same window.
- A directory outside any repository runs no git process (counted).
- The breadcrumb chip shows `main`, then a detached short hash after `git checkout <hash>`, with no process spawned for the chip itself.
- A repository with 200,000 tracked files: status scoped to a 100-file subdirectory returns under 300 ms warm.
