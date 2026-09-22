# 12 — Search

**Status:** built and tested as trimmed.

Builds on: `01-daemon-and-listing.md` (string pool, windows, watch), `02-shell-and-views.md` (search box), `06-remote-locations.md` (remote walks).

## As built — the plan trimmed (2026-09-21, D5)

The choice was: build this plan as written (5–7 days) or keep what works, fix what is wrong and say so. The second. What differs from the sections below, in one place so nobody has to read them against the code:

- **Two surfaces, not one field.** *This folder* is the **filter strip** above the listing (`/`, `Ctrl+F`): it sends `Filter` on the open listing, the view does not change, and the strip reads `12 of 458`. *Everywhere* and *a location* are the **search overlay** (`Ctrl+Shift+F`, `views/SearchOverlay.qml`) — a panel over the window, so closing it leaves the folder, selection and scroll exactly as they were, rather than the results replacing the pane.
- **The scope is a chip you click**, one per scope across the top of the overlay (Everywhere, then each saved location). **No typed prefixes, no scope menu, no `Tab` cycling**: `everywhere:` and `homelab:` are parsed by `ui/SearchBox.qml`, which only the portal chooser uses, and there with no scopes to offer. A keystroke restarts a debounce timer rather than cancelling a request in flight.
- **The index is a flat scan, not a sorted one.** There is no `sorted` array and no memory map: every mode walks the names once (`prefix` compares the case-folded key's start, `fuzzy` falls back to a subsequence match) and keeps the best 10,000 in a heap. `index.bin` is a plain little-endian dump, read at startup.
- **A location search is not streamed.** One recursive `Scan` where the plugin has one, folder by folder where it answers `Unsupported` (FTPS; SFTP without a shell), collected and answered whole — and *known and left*: it runs in the client's request thread, so that window's other requests wait for it.
- **No `IndexProgress`** — the event is in neither the daemon nor the shell, and leaves `API-DAEMON.md` too (D10).
- **The two bugs found by the audit are fixed, with tests** (2026-09-19): the index ranked only the first 40,000 matches it met, so the best hit could be one it never reached — every match is ranked now, and `capped` means more than fit rather than exactly as many; and a location whose plugin has no recursive scan found nothing at all instead of being walked folder by folder.

None of the Verification budgets below has been measured.

## Goal

Search that feels instant at every scope: filtering the current folder as you type, finding any file under home in a few milliseconds, and walking a remote location without blocking. Name search only in 0.1.0; content search is a later plugin.

Mockups: `SearchEverywhere.dc.html` (results replace the pane, scope chip in the field) and `SearchFolder.dc.html` (the open listing filtered in place, scope menu open) in `docs/design/`.

## Scopes

The scope lives inside the search field; there is no separate control. Plain text searches **This folder**. Typing `everywhere:` (or `all:`) or a location name followed by a colon (`homelab:`) switches scope, and the typed prefix collapses into a chip at the left of the field. The chip, or a small chevron when no scope is set, opens a **scope menu** listing This folder, Everywhere (with the index size), and every connected location, with a hint line showing the prefix syntax. `Tab` cycles scopes as a keyboard shortcut; Backspace on an empty query removes the chip. Prefix matching is case-insensitive and only recognised at the start of the field, so a filename containing a colon still searches normally after the first character.

**This folder**: a `Filter` request on the open listing. The daemon matches against the case-folded keys already in the string pool with a SIMD substring scan; each keystroke narrows the previous match set. No index, microseconds. The view does not change: the same icon, list or columns view shows only the matching rows with the match highlighted, the shortcut bar reads "N of M items match", and `Esc` restores the full listing with the selection kept.

**Everywhere**: the name index below. Results open as a listing under a new `lid` and scroll through the normal window cache.

**This location** (remote): a recursive walk through the plugin's `Scan`, breadth-first, streamed into a results listing as directories come back, cancelled when the query changes. Inline metadata from SFTP and FTPS means results carry sizes and dates without extra requests. On SFTP with exec acceleration (plan 06) the walk is a single streamed `find` over the tree, so the first results arrive within a round trip and the whole tree is searched in seconds.

## The name index

Lives in kikid (`src/index/`), built from the same phase-1 scanning as listings. No separate service: a separate process would add a hop and duplicate the pool code.

**Layout**, one memory-mapped file `~/.cache/kiki/index.bin` with a header, then:
- `names`: one string pool of every entry name under the roots.
- `entries`: per entry `{ name_off: u32, name_len: u16, kind: u8, parent: u32 }`. Directories are entries too; the root's parent is itself.
- `dirs`: per directory `{ entry: u32, mtime_ms: u64, first_child: u32, child_count: u32 }` so a directory's children are a contiguous range.
- `keys`: case-folded, natural-order keys for every name, and `sorted`: a `u32` index array over `keys` for prefix and exact lookup.

A million entries is roughly 20 MB of names plus 12 MB of entries and keys. The daemon keeps it in memory and saves it to `~/.cache/kiki/index.bin` (a plain little-endian dump of the vectors, written atomically) after every build and every directory walk; at startup the file is loaded when its roots still match the settings and brought up to date by the walk, so a restart costs one read of the file plus a stat per directory instead of a full crawl. Memory-mapping the file for queries is a later step; a read at startup is under a second for a million entries.

**Roots and excludes**: `$HOME` by default; roots and excludes are edited on Settings → Search. Excludes as built: `.cache`, `.git`, `node_modules`, `__pycache__`, `.Trash`, `target`, and any directory containing a `.kiki-noindex` file. ~~the mirror filter rules~~ (the mirror's rules are its own; the two lists are not shared). Configurable in `settings.toml` under `[index]`.

**Building**: a phase-1 crawl of the roots on one low-priority thread with a 1 MB `getdents` buffer, names only, never stat on files. A home directory with a million files builds in a few seconds warm. The first build starts 30 s after the daemon's first client connects, so it never competes with the first window.

**Staying fresh**:
- Directories with a live watch (any the user has opened, plan 01) patch the index in place through the same `Changed` events that patch listings.
- Everything else is covered by a **directory walk**: at startup and every 10 minutes, stat every directory in the index (`statx` relative to its parent fd, mtime only) and relist only those whose mtime changed. A directory's mtime changes on every add, remove or rename inside it, so this catches all name changes without file stats. Walking a million-file tree's directories is a few hundred milliseconds warm. New directories found in a relist are crawled.
- Whole-tree notification (fanotify on a filesystem mark) needs root and is not used.
- A query against an index that is mid-rebuild runs against the old mapping; the swap is atomic.

**Querying**: `Search { lid, scope: "everywhere", query, mode }`.
- `mode: "substring"` (default): parallel scan of `keys` in chunks across the worker pool, collecting entry ids; a million names in under 5 ms.
- `mode: "prefix"`: binary search in `sorted`.
- `mode: "fuzzy"`: subsequence match with fzf-style scoring, over the substring candidates when the query has three or more characters, else over everything.
- Ranking: exact name, then prefix, then substring; ties broken by depth (shallower first) and by a small boost for directories the user opened in the last week (from the daemon's visit log). At most 10,000 results are materialised; the count reports `capped: true` beyond that.
- Results are a listing: rows are `Row` plus `parent: Uri`, served by `Window`. The results view replaces the pane content: two-line rows (name with the match highlighted, parent folder beneath in muted text), kind icon or thumbnail, modified and size columns, a header line with the count, query, scope and index age. `Enter` opens the item, `Ctrl+Enter` reveals it in its folder, `Esc` returns to the folder the pane showed before the search. `Enrich` and thumbnails work on results like any listing.
- Each keystroke cancels the previous query; the results listing is reused (`Reset`), not reopened.

## Protocol additions

| Request | Fields | Reply |
|---|---|---|
| `Search` | `lid`, `scope: "everywhere" \| "location"`, `uri` (for `location`), `query`, `mode` | `{ n, capped, indexAge: u64 ms }` then `Count`/`Reset` on the `lid` |
| `IndexStatus` | | `{ entries, dirs, roots: [Uri], builtAt, refreshing: bool }` |
| `IndexRebuild` | | `{}` |
| `IndexRoots` / `SetIndexRoots` | — / `roots: [Uri]` | `{ roots }` / `{}` |

~~Events: `IndexProgress { done, total }` during a build or walk.~~ **Amended 2026-09-21 (D10):** never built, never emitted; out of `API-DAEMON.md` too. The Settings → Search page shows entry and directory counts and the index's age from `IndexStatus`, which is enough to know where it stands.

**IPC added**: ~~`searchScope(folder|everywhere|<location>)`, `search(text)` accepts the same prefix syntax as the field.~~ **As built:** `shell search <text>` filters the open listing through the strip, and `shell searchEverywhere <text>` opens the overlay on that text (empty closes it). The scope is not settable from IPC.

## Verification

- Index build of a generated tree with 1,000,000 files and 100,000 directories completes under 10 s warm; the file is under 64 MB.
- Substring query over that index returns under 10 ms; prefix under 1 ms; fuzzy with a 3-character query under 50 ms.
- Creating a file in an unwatched directory appears in results after the next walk; the walk over that tree stats directories only (counted) and finishes under 1 s warm.
- Creating a file in a watched directory appears in results within 100 ms.
- Typing a 10-character query one key at a time issues 10 `Search` requests, each cancelling the last, and the results listing is never reopened.
- Remote walk over a 20,000-file SFTP tree streams first results within 300 ms and makes no `Stat` calls.
