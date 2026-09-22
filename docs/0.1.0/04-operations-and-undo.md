# 04 — Operations and undo

**Status:** built and tested; parallel small-file copy not in 0.1.0; activity → plan 32.

Builds on: `01-daemon-and-listing.md`, `03-inspector.md`.

Mockups: the context menu in `IconView.dc.html`, the toast in `ListView.dc.html`.

## Goal

Every write goes through one job queue with progress, cancellation and a journal, so Ctrl+Z undoes it. This plan is the foundation for archives, transfers and the mirror engine.

## Design

**Job queue** (`jobs` module in kikid):
- `Job { id, kind, status: Queued | Running { done, total, bytes } | Done | Failed(msg) | Cancelled, journal: Option<Inverse> }`.
- N worker slots (default 3); one job per slot; cancellation via a `CancellationToken` checked between actions and inside byte copies. A job that does its own internal concurrency (a mirror run, plan 08) still occupies one slot.
- Op set for `Submit`: `Copy { items, dest }`, `Move { items, dest }`, `Rename { path, name }`, `Trash { items }`, `Delete { items }` (remote, confirmed, not journaled), `Mkdir { path }`, `Chmod { items, mode, recursive }`, `Compress { items, archive, format }` and `Extract { archive, dest }` (plan 05), `MirrorScan` and `MirrorRun` (plan 08). **As built** (`jobs.rs`, amended 2026-09-21) the set also carries the inverses and the trash's own ops as kinds of their own — `restore`, `emptyTrash`, `deleteCopies`, `movePairs`, `rmdirIfEmpty`, `chmodList` — the last three hidden from Activity, since they are the machinery of an undo rather than something a person asked for. `items`, `dest`, `path` and `archive` are URIs, so a copy between two backends is the same op as a local copy, and the daemon resolves each URI with the plan-01 resolver.
- Progress is pushed on a `JobEvents` subscription every 100 ms or on state change.
- Jobs outlive windows: the daemon keeps them; a reopened window reattaches.

**Operations** and their recorded inverse:

| Operation | Inverse |
|---|---|
| Copy | delete the copies |
| Copy **to a server** | `deleteCopies` (below) |
| Move | move back |
| Move **that touches a server** | none — deliberately not undoable |
| Rename | rename back |
| Trash | restore from `~/.local/share/Trash` (FreeDesktop spec, `info/*.trashinfo`) |
| New folder | delete it if still empty |
| Chmod (from the inspector) | restore the recorded modes |

**Undo across machines** (added 2026-09-21, owner: "build undo for copy to a server only"). A copy whose destination is a server journals `deleteCopies`: every file it created, with the size and time **the server** reported as it landed, and every folder it created — never the folder it landed in, never a skipped item. The undo deletes only what the server still says is that, by the check the mirror makes about that server (`pick_detector`: SFTP size and time, FTPS size alone), leaves what has changed or gone and names it, and removes a folder only if it is empty afterwards. Both toasts say a remote delete is permanent: *"Copy site to www — Undo deletes it from homelab, permanently"*, *"Undid copy — 3 items deleted from homelab (permanently), 1 item left because it had changed"*. A copy cancelled or failed half way journals the part that arrived; it survives a daemon restart; redo copies again. A **move** that touches a server stays outside undo. (`kikid/tests/undo_remote.rs`, `remote_transfers.py`.)

Copies of large trees use `copy_file_range` in a loop until all bytes are written (it may return short), falling back to a 1 MB buffered copy when it returns `EXDEV` or `ENOSYS`. Before a large copy the destination is `fallocate`d to its final size (no fragmentation, early `ENOSPC`) and the source gets `posix_fadvise(SEQUENTIAL)`; after it, `POSIX_FADV_DONTNEED` on the source so a big copy does not evict the page cache. ~~A job with many small files splits into up to N parallel workers (the job's own pool, like a mirror run), since small-file copies are latency-bound.~~ **Not in 0.1.0** (D22, amended 2026-09-21): a local copy runs one worker, and phase 4's measured throughput (73 MB/s up, 29–35 down over SFTP; a 415 MB local copy over in 0.3 s where the filesystem clones) says it can wait. A mirror run is the one job with a pool of its own. Cross-device moves are copy then delete. Name collisions prompt once per job (replace, keep both, skip, apply to all).

**Journal**: an ordered stack per daemon, capped at 100 entries, persisted to `~/.local/state/kiki/journal.json` so undo survives a daemon restart within the same session. Undo pops the top entry and submits the inverse as a job; redo (`Ctrl+Shift+Z`) re-runs the original. An inverse that cannot apply (the target changed since) fails loudly and stays on the stack.

**Context menu**: Open, Open with…, Copy, Cut, Paste, Move to…, Rename, Compress… (plan 05), Extract here / Extract to… (plan 05, archives only), Copy path (the local path for `file://`, the full URI otherwise), Open in ▸ (plan 14), Share ▸ (plan 18), AI ▸ (plan 19, text files and folders), Move to Trash. Keys as in the mockup. The same list in every view: columns view's row menu had fallen behind list view's and was brought level on 2026-09-21.

**Open with… takes a selection** (2026-09-21): `OpenWith` is asked about all the chosen files at once and answers with what opens **all** of them — a text file and a Markdown file share Code and Neovim, a text file and a picture share nothing and the row says "No application opens all of these" — and the app chosen is handed the lot. An app whose `Exec` takes one file (`%f`, `%u`) is run once for each, as the Desktop Entry spec has it.

**A server's files have no trash** (2026-09-21). `Del`, the menu's Move to Trash and a drop on the sidebar's Trash are one function, so they cannot drift apart: this machine's files go to the trash without asking; a server's get *"Delete permanently? — a.txt is on homelab, which has no trash. It will be deleted for good, and this cannot be undone."* in danger colours, and a `delete` job on Yes. A mixed selection does both. The daemon's refusal of a remote `trash` stays as the guard for anything that sends one anyway.

**Toast**: one line per completed destructive job ("Moved wallpapers.zip to Trash") with an Undo button, dismissed after 8 s or on the next job.

**Activity**: the orb at the bottom right of the window, the popup it opens, and each job's log — `32-activity.md`. (This plan's "click the status bar's progress area" was an unmarked 200 px hit area; it is gone.)

**Protocol**: `Submit { op } -> JobId`, `Cancel { id }`, `Jobs -> [Job]`, `Undo`, `Redo`, `JobEvents` subscription (one small message per state change or 100 ms of progress), `Collision` prompt round-trip. The shell keeps a `Jobs` `ListModel` patched from `JobEvents`; job messages are a few hundred bytes, so parsing them in QML is free.

**IPC added**: `undo()`, `redo()`, `activity()`, `contextMenu(action)`.

## Verification

- Each operation has a round-trip test: run, undo, tree byte-identical to before (`diff -r`).
- Cancelling a 1 GB copy stops within 200 ms and leaves no partial file.
- Trashed files appear in the Trash favorite (`trash:///`, a listing of `~/.local/share/Trash/files` served by the same window machinery) and restore to their original path: `TrashInfo` gives the original path and deletion date per name, `Enter` or the context menu submits `restore { names }` (undoable back to trash), `Del` submits `delete` on the `trash:///name` URI (permanent, confirmed by the key itself), and Empty Trash submits `emptyTrash` (not undoable). Paste, New folder and drops into the trash listing are disabled.
- Undo after a daemon restart works for the last entry.
