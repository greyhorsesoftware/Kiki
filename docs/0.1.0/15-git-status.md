# 15 — Git status

**Status:** built and tested.

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

**Big repositories**: the pathspec keeps a status run proportional to the subtree in view. ~~On repositories over 50,000 tracked files the daemon suggests, once, enabling `core.untrackedCache` and `core.fsmonitor` in a toast with a one-click `git config`.~~ **Amended 2026-09-21: no such toast, and kiki will not suggest that one.** A repository's own `.git/config` can name programs git then runs, so kiki runs git with `core.fsmonitor=` and `core.hooksPath=/var/empty` — listing a folder must not execute what a repository asks for (`git.rs`, tested). What is built is the other half: if a status run exceeds 2 s the directory is marked slow (`git::is_slow`) and is not re-run on every command in a terminal.

**Branch chip**: `.git/HEAD` is read directly (a symbolic ref or a detached hash), so the chip costs one small file read per navigation and no process. Ahead and behind come from the status header when available.

## UI

| Surface | Shows |
|---|---|
| List and columns rows | a one-letter badge after the name: `M` modified (yellow), `A` added (green), `D` deleted (red), `R` renamed (yellow), `!` conflicted (red), `?` untracked (muted green); ignored files and folders dimmed. **The colours are the theme's, not those words** — see "Colours are meanings" below; they were Tokyo Night hex literals until 2026-09-21 |
| Icon tiles | a coloured dot at the top-right of the icon in the same colours; ignored dimmed |
| Folders | the aggregated state as a mark; a folder that is **itself a repository** shows a **branch capsule** instead — `⎇ main`, or the short hash on a detached HEAD — where the one-letter badge goes, on the same line, coloured by that repository's own aggregate state: one element says both which branch and whether it is dirty. Icon tiles carry that colour on the dot they already draw; columns rows carry the capsule as list rows do. A long branch elides inside the capsule, which takes at most 40 % of the name column. `folders = "off"` takes it away with the other folder marks. *(Amended 2026-09-21, owner: "why not just make that a branch capsule that has same colors as dot and shows branch too?" — this replaces "the branch name under its name", which wanted taller rows.)* |
| Breadcrumb | a chip `⎇ main ↑2 ↓1` at the right end of the path when the pane is inside a repository; click copies the branch name |
| Inspector, General tab | Git: state, branch, last commit (short hash, author, relative date, subject) |
| Sidebar Favorites | ~~a small dot on a favorite that is a dirty repository root~~ **Not in 0.1.0 (2026-09-21)**: not built, and not asked for. The sidebar's only dot is the green one a *connected* location wears |

Settings (`[git]` in `settings.toml`): `enabled = true`, `showIgnored = "dim" | "hide" | "normal"` (the key is camel-cased, as every other setting is), `folders = "aggregate" | "off"`. `hide` is the daemon's: an ignored row is left out of the listing the way a dot-file is, from that folder's next listing on.

Not in 0.1.0: staging, unstaging, discarding, commit, diff view, submodule recursion, status on remote locations. Context-menu git actions are a later plan.

## Protocol additions

`Row` gains `git: { state: "modified" | "added" | "deleted" | "renamed" | "conflicted" | "untracked" | "ignored" | "clean", staged: bool } | null` (`null` outside a repository or before status has run).

| Request | Fields | Reply |
|---|---|---|
| `Repo` | `uri` | `{ root: Uri, branch: string \| null, detached: bool, ahead: u32, behind: u32, dirty: bool } \| null` |
| `GitStatus` | `uri` | `{ state, staged, branch, last: { hash, short, author, time, subject } \| null }` (replaces plan 03's shape) |
| `GitRefresh` | `uri` | `{}` (re-run status for that directory now) |

Event: `RepoChanged { root }` when HEAD or the index changes, so the breadcrumb chip updates.

**IPC added**: ~~`gitState(name)` for tests.~~ **Amended 2026-09-21: not built.** A row's git state is read out of `shell state` and the listing's rows, which is what `git_status.py` asserts on; a badge's colour is read off the item by `tst_GitBadges`.

**Mockups**: add badges to `ListView.dc.html` rows and dots to `IconView.dc.html` tiles, plus the breadcrumb chip on `Main.dc.html`, before building.

## Verification

- In a test repository with one file of each state (modified, staged, deleted, renamed, conflicted, untracked, ignored), every row shows the right badge and colour, and the folder containing them shows the aggregated dot.
- The listing paints before status runs; status for a 10,000-file subtree lands within 500 ms warm and is pushed only to rows in the live window.
- Editing a tracked file updates its badge within 400 ms without re-listing; `git commit` updates the branch chip and clears badges within the same window.
- A directory outside any repository runs no git process (counted).
- The breadcrumb chip shows `main`, then a detached short hash after `git checkout <hash>`, with no process spawned for the chip itself.
- A repository with 200,000 tracked files: status scoped to a 100-file subdirectory returns under 300 ms warm.

## The branch capsule (added 2026-09-21)

**Why it was needed.** A listing's git state comes from the repository the *listed* folder is in. A folder like `~/Projects` is in none, so the rows that most want a mark — the projects themselves — had nothing at all.

**Repository-root rows.** Each folder row is checked for a `.git` of its own (one `stat`; for a worktree or a submodule it is a file, whose `gitdir:` is followed) and its branch read straight out of that repository's `HEAD` — one small read, no process, so the branch arrives with the listing. The state is a second, cheaper `git status --porcelain=v2 -z --untracked-files=normal` per repository, reduced to its worst state: never on the listing's path, at most four in flight across the daemon, remembered while that repository's `HEAD` and index sit still. A repository inside a repository shows its own branch and state. **Measured on forty small repositories**: time-to-listing 2.3 ms with the capsules and 2.3 ms without; all forty coloured 13 ms later. A test fails if `git` ever creeps onto the listing's path.

**Freshness without a watch.** Forty projects cannot have forty watches — there are 64 for the whole daemon. The watcher's one-second sweep stats each repository row's `HEAD` and index instead (two stats a project, only while somebody is looking) and re-works the rows whose stamps moved: a commit, a checkout or a `git add` from a terminal reaches the capsule within about a second. *Known and left*: an edit in a project's working tree moves neither file, so that colour waits for the next `HEAD` or index move, for the folder to be opened again, or for `GitRefresh`.

**Protocol.** On a folder that is itself a repository, `Row.git` also carries `root: true`, `branch: string` and `detached: bool`, and its `state` is that repository's own aggregate (`clean` until the aggregate has run, which arrives as a `Rows` update). No other row's `git` changes.

**Colours are meanings.** Badges and capsules take the theme's colours (they were Tokyo Night literals until 2026-09-21), with two guards: *danger* is the theme's red unless that red is not red, and **changed** is the theme's yellow unless that yellow is red — matte-black's is `#b91c1c`, which drew a modified project in the colour of a conflicted one (`Theme.changed`, `tst_ThemeDanger`).

**Found on the way, fixed**: the breadcrumb's branch chip was blank or garbage in a worktree or a submodule (it read `<root>/.git/HEAD` literally); `push_rows` could panic with a stale row index after a rescan.

**Where it sits**: at the right edge of the name column, where the letter badge has always been, so the branches line up as a column. Tests: `git.rs` (4), `listing/tests.rs` (4), `tst_GitBadges` (+12), `tst_ColumnsCapsule`, `git_status.py` (+9, 18 in all); a picture at `tests/e2e/out/git-capsule.png`.

