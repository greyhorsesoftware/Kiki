# 06 — The index keeps up with what you are looking at

**Status:** built, 2026-10-02 — L1 to L3. The name index (`index.rs`) was rebuilt every ten
minutes (`REFRESH_EVERY`, 600 s) and between walks did not change: a folder made a minute ago
was not in Search everywhere. Now what a window is showing reaches the index as it changes.

## As built

- **What the watch sees, the index gets** (`index::patch_names`, called from
  `Listing::patch`): the names a watched folder's patch found — stat'ed already, so the kind
  comes with them — are appended under the folder's entry, removed ones tombstoned with their
  subtrees; a rename is the two. A folder that arrives is listed one level, and its own
  subfolders are put down with an mtime of nought, "known but not yet listed", which is how the
  refresh finds them without a structure of its own. Measured in `kikid/tests/index_live.rs`:
  a file written into a watched folder is searchable **61 ms** later (the watcher's 50 ms
  coalescing and its 25 ms poll); a renamed folder's children are found under the new name
  within the same second; what was two levels down comes with the next refresh.
- **The refresh was already incremental** — `refresh_walk` stat'ed every known folder and
  re-listed only those whose mtime had moved; the plan's "still visits every folder" was wrong
  and is struck. What was missing was the number: `kikid bench` now has `index_refresh_ms`
  (and `index_refresh_relisted`) beside `index_build_ms`. On an unchanged `flat200k`, pinned:
  **build 43.2 ms, refresh 1.34 ms**, nothing re-listed — 2 000 stats against 202 000
  entries read. The stat pass runs under the read lock (`Index::stale_dirs`), the re-listing
  under the write lock (`relist_dirs`), so a search is answered while the folders are asked
  the time.
- **A burst is a batch.** The watcher already coalesces fifty milliseconds of events per
  folder into one call; the index takes that call as one lookup of the folder and *n*
  appends, not *n* lookups. Five thousand files written into a watched folder cost the index
  **one patch** and no re-list (`index::work()` counts both, for the test). Past 5 000 in one
  batch — or an inotify overflow — the listing rescans, and now tells the index to re-list the
  folder once (`patch_dir` from `rescan`), which it did not before.
- **`IndexStatus` has `live`**: entries put in by watched folders since the last refresh or
  build; a refresh folds them in and it reads nought again. `API-DAEMON.md`, `API-DELTA.md`.

The interval stays ten minutes, as decided.

## Today

- The daemon already watches the folders the windows are showing (`watch.rs`, inotify,
  `MAX_WATCHES` 64, least-recently-registered evicted) and patches their listings live.
- The index is name-only, one entry per file or folder with a parent link (`push(name, kind,
  parent)`), rebuilt whole (`rebuild_async`), saved to `index.bin`; `find_dir(path)` walks it
  from the root that contains a path; `dir_mtime` remembers each folder's mtime so a refresh
  can skip folders that have not changed (`index.rs:491`).
- A query is a scan of every name (`query`, ranked, `MAX_RESULTS` 10 000), ~400 µs for 200k
  entries pinned (`26-benchmarks.md`).

## Decisions

1. **What is watched is indexed as it changes.** The watcher's events (`Created`, `Removed`,
   `Renamed` — the ones the listing patches already carry) are also handed to the index: a
   `Created` in a watched folder appends an entry under `find_dir(parent)`; a `Removed` sets
   its `removed` flag (the array exists); a rename is both. A new *folder* is walked one level
   (its children are what the next search wants) and its subfolders left for the refresh.
   No new structure: the index is append-and-tombstone already, with the refresh as the
   compaction.
2. **The refresh becomes incremental by mtime, which it half is.** `dir_mtime` is kept but the
   walk still visits every folder; the refresh should `statx` each known folder and descend
   only where the mtime moved (a folder's mtime changes on any entry added or removed in it).
   `flat200k` refresh then costs 2 000 stats instead of 200 000 readdirs — measured in the
   bench (`index_refresh_ms`, a new line beside `index_build_ms`).
3. **The interval stays ten minutes.** With 1 and 2, what you are looking at is always
   current and the rest is at most ten minutes old, which is what the plan-12 design accepted.
   A `SetIndexRoots` still rebuilds whole.
4. **Bounded the same way:** an entry appended live counts toward the same memory as a walked
   one; the tombstones are compacted on the next refresh; a burst of events (a build writing
   ten thousand files into a watched `target/`) is coalesced per folder — one relist of that
   folder at the next tick, not ten thousand appends — using the coalescing the watcher
   already does for listing patches.

## Levels

- **L1** (a day): decision 1 with the tests — a file created in a watched folder is found by
  `Search` within a second; removed, it is not; a renamed folder's children are found under
  the new name.
- **L2** (half a day): decision 2 and its bench line; the refresh of an unchanged `flat200k`
  under 50 ms.
- **L3** (an hour): decision 4's coalescing, with a test that writes 5 000 files and checks
  the index took one relist.

No wire change; `IndexStatus` gains a `live` count for the Settings page's index line, which
is one word in `API-DELTA.md`.
