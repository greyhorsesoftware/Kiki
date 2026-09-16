# 01 — Daemon, string pool and windows

Builds on: nothing. Everything else builds on this.

## Goal

A lean Rust daemon (`kikid`) that enumerates any directory into a string pool without stating files, serves the rows a view can see through window requests, fetches metadata only for those rows, keeps listings current with inotify, and caches everything. The proof is a pure-QML Quickshell window scrolling a 200,000-entry directory at 60 fps.

## Design

**Crate layout** (`kikid/`, one binary, minimal graph per CORE.md):
- `src/json.rs` — writer and reader for this protocol's messages. The reader accepts exactly the grammar the protocol uses (objects, arrays, strings, integers, booleans, null) and is fuzzed.
- `src/proto.rs` — message types and framing as specified in `API-DAEMON.md` (`u32` LE length, `u8` type, payload; type 0 JSON, type 1 binary).
- `src/vfs/` — the `Backend` trait, `local.rs` (rustix: `getdents64`, `statx`, `openat`, `renameat`, `unlinkat`, `mkdirat`, `copy_file_range`, `fchmodat`, `utimensat`), and `uri.rs` (the resolver).
- `src/string_pool.rs` — the listing string pool.
- `src/listing.rs` — phase 1 scan, phase 2 windows, sort and filter, the listing cache.
- `src/watch.rs` — inotify via the kernel API on one fd, one watcher thread.
- `src/server.rs` — Unix socket at `$XDG_RUNTIME_DIR/kiki.sock`; a reader thread and a writer thread per connection, events fan out through a channel to the writer.

**URIs and the resolver**: `Uri` is a validated newtype (scheme, authority, percent-decoded path). `resolve(uri) -> (BackendRef, PathBuf)`: `file://` and bare absolute paths resolve to the local backend; other schemes resolve to a plugin process by authority (plan 06). `parent()`, `join(name)` and `display()` (`~`-shortened for local, `name/path` for a location) feed breadcrumbs.

**`Backend` trait** (local implements it directly; plugin processes implement it by protocol, plan 06):

```
scan(path) -> stream of (name, kind) chunks          phase 1: never stats
stat(path) -> Meta                                    phase 2: size, mtime_ms (0 = unknown), mode, uid, gid, digest
read(path) -> byte stream        write(path) -> byte sink
mkdir, rename, delete (file or empty dir), set_mtime (best-effort), chmod (when `mode` capability)
capabilities() -> { trash, set_mtime, mode, real_dirs, digest_kind, separator }
```

**String pool** (`string_pool.rs`): one `Vec<u8>` of names, a `Vec<u32>` of offsets, a `Vec<u8>` of kinds (`Dir`, `File`, `Link`, `Other` from `d_type`, plus an extension-derived kind byte for icons), and a `Vec<Option<Meta>>` filled by phase 2. Sort orders are `Vec<u32>` index arrays over the string pool, one per (role, order) requested; a filter is a `Vec<u32>` of matching indices. A 200,000-entry directory is a few megabytes and never reallocates names.

**Two-phase virtualized readdir**:
- *Phase 1.* The scanner thread calls `getdents64` into a 1 MB buffer (a few syscalls for 100,000 entries) and appends names and kinds to the string pool in chunks of ~1024, publishing the count after each chunk so windows can be answered before the scan finishes. No `stat`, no content sniffing. While appending, the scanner also writes a compact **sort key** per name into the pool (case-folded, digit runs encoded for natural ordering) so sorting never touches the names again. The directory fd stays open for the lifetime of the listing; phase 2 stats relative to it. A 100,000-entry directory completes in well under a second; the first chunk is available within a frame.
- *Sorting.* Index arrays are built with `sort_unstable` over the precomputed keys; above 50,000 entries the sort splits into chunks across `std::thread::scope` threads and merges. The first window is answered in directory order while the sort runs and a `Reset` follows when it lands, so a huge directory paints immediately and reorders once. A sort order, once built, is cached with the listing.
- *Phase 2.* A `Window` request names a range of rows in the current sort order. The daemon answers immediately with what it has (names, kinds, any cached `Meta`) and queues `stat` for rows in the range that lack it, nearest the range's centre first, on the stat worker pool. Stats are `statx(dirfd, name, AT_SYMLINK_NOFOLLOW | AT_STATX_DONT_SYNC, mask)` relative to the open directory fd with only the fields the UI shows in the mask, so no path is re-resolved per entry and network filesystems are not forced to revalidate. The daemon remembers each client's current window; when stats land it **pushes** `Rows { id, first, rows }` for the part of that window that changed, so there is no refetch round trip. Rows outside any requested window are never stated. Requests for a range that has scrolled away are cancelled.
- *Free metadata.* A backend whose `scan` can return metadata at no extra cost (SFTP readdir carries attributes; FTPS `MLSD` does; a future S3 listing does) fills `Meta` during phase 1, so remote windows and mirror scans never pay per-file round trips. Local `getdents64` cannot, so local stays two-phase.
- *Operations that need everything* (sort by size or date, size totals, mirror scan) ask for `Enrich { id }`, which stats every row at low priority and reports progress; sorting by name or kind never waits for it. Name search needs only phase 1. Directories under 2,000 entries are enriched fully right after phase 1.

**Listing cache**: completed string pools stay in memory keyed by URI in an LRU capped at 500,000 entries in total. A watched directory is authoritative: inotify events patch its string pool, so reopening it, opening it in a second pane or re-expanding a column is served from memory with no directory read. When a watch is evicted (64 per client, LRU) the string pool is kept but marked stale; the next open re-runs phase 1 and diffs against the old string pool so unchanged rows keep their `Meta`. At startup, and after idle, the daemon prefetches Favorites and the last-open directories so the first window of a session is usually served from memory. Remote listings (plan 06) have no watch and cache for 30 s, refreshed on expiry, on `F5`, or immediately after a job writes there.

**Allocator tuning**: at startup kikid calls `mallopt` three times through `libc`: `M_MMAP_THRESHOLD` pinned to 131072 so the dynamic threshold never rises and freed large buffers return to the kernel, `M_ARENA_MAX` 2 so a dozen threads do not retain a dozen heaps, and `M_TRIM_THRESHOLD` 1 MiB so the main heap trims promptly. To keep the pinned threshold cheap, buffers used in loops are allocated once and reused: the `getdents` buffer per scanner, the copy buffer per job, the decode buffer per thumbnail worker. Kept only if the measurement below shows it matters.

**Watch**: one inotify fd, one thread, events coalesced for 50 ms into a per-directory batch of added, removed and modified names. The batch patches the string pool in place: removed names are tombstoned, added names appended with their kind, modified names lose their `Meta` and thumbnail so the window re-stats them; then the view is rebuilt and `Reset` sent. A queue overflow or a batch above a few thousand names falls back to a rescan that keeps `Meta` for names still present. The same batch patches the search index for that directory (plan 12).

**Protocol** (added by this plan; every message has a request id):
- `Open { id, uri }` with a client-chosen id, so `Open` and the first `Window` go out in one write; then `Count { id, n, done }` events as phase 1 progresses
- `Window { id, first, count } -> { rows: [{ name, kind, isDir, isLink, meta? }] }` in the current sort and filter; the daemon records this as the client's live window
- `Rows { id, first, rows }` (event: pushed rows for the live window after phase 2 lands, a thumbnail arrives or a watch patch)
- `Sort { id, role, order }` and `Filter { id, text }` -> `Reset { id, n }`
- `Enrich { id }` -> `Progress { id, done, total }`
- `Close { id }`
- `Stat { uri } -> Meta`
- `Prefetch { uri }`: open and phase-1 scan without a client window, for the shell to warm the parent, Favorites and, in columns view, the selected folder
- `Ping`, `Version`

Errors are typed (`NotFound`, `Denied`, `Io(msg)`), never strings the UI has to parse.

**QML side** (the shell's `WindowCache`, a plain QML object used by every view in plan 02): holds `count` from `Count` events, a map of row index to row for the current padded window (visible range plus 200 rows in the scroll direction, 100 behind), issues `Window` on scroll with a 16 ms debounce, applies pushed `Rows`, and clears on `Reset`. It requests only the rows it does not already hold. Delegates use `required property` bindings, `reuseItems: true` so scrolling recycles delegates instead of creating them, `Image { asynchronous: true; sourceSize }` so a 128 px thumbnail is never decoded larger than its slot, and render a placeholder for `meta` until it arrives. The shell sends `Open` and the first `Window` in one socket write on navigation. `JSON.parse` on a 60-row window is microseconds; that is the whole reason this design holds.

## Verification

- `json.rs` round-trips every protocol message, rejects malformed input without panicking, and survives a fuzz run of 10 minutes.
- Bench binary on a 10,000-entry directory: first phase-1 chunk under 16 ms, full phase 1 under 100 ms.
- 200,000 entries: phase 1 under 1 s; first window painted in directory order within a frame and the sorted `Reset` within 300 ms; a `Window` request at any offset answered under 5 ms; visible rows show size and date within 100 ms of stopping a scroll; the count of `statx` calls equals the rows that were ever in a window, not the directory size.
- Quickshell test page scrolling 200,000 rows through `WindowCache` stays under 16 ms per frame with no frame blocked on a request, with delegate creation count flat after the first screen (reuse working).
- Navigation to a directory whose parent was prefetched sends one socket write and paints from the daemon's cache with zero directory reads.
- `touch` in a watched directory changes the row within 100 ms and contains exactly that entry.
- Reopening a watched 10,000-entry directory performs zero directory reads (counted by the bench binary); after watch eviction the revisit re-lists but keeps `Meta` for unchanged rows.
- Two clients on the same directory both receive every `Changed` event.
- Resolver tests: `file:///tmp/x`, `/tmp/x` and `~/x` resolve to the local backend; an unknown scheme is a typed error; `parent`, `join`, `display` round-trip.
- Resident memory after opening and closing a 200,000-entry directory (with its cache entry evicted) and after a 1 GB copy returns to within 5 MB of the idle baseline; with the `mallopt` calls removed it does not. If both pass, the calls go.
- Unit tests for the local `Backend` on a temp tree (scan, stat, mkdir, rename, delete, set_mtime, chmod).
