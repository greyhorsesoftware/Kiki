# 32 — Activity: the orb, the popup, the job log

**Status:** built and tested (2026-09-20). Not built: **Save…** in the log window (Copy all is there); a mirror's per-file bar when it runs several workers.

Builds on: `04-operations-and-undo.md` (every write is a job), `06-remote-locations.md` and `08-mirror.md` (the jobs worth watching), `29-release-readiness.md` section I, which placed the orb. This document replaces `activity-view-spec.md`, which was written for another toolkit's toolbar button; what survived of it is here, and the file is gone so that the tree holds one description of the feature.

## Goal

One place to answer "is it still going, how far along, and did it work?" — and, when it did not, "what exactly happened on the server?".

## The orb

At the bottom right of the window, at the right end of the bottom bar — the window's, not the key-hint row's, so it is there in project mode's narrow tree and in the gallery. **Always there**: a control that comes and goes is one nobody can learn.

| State | Looks | Tooltip |
|---|---|---|
| nothing running, nothing failed | a dim, still dot | "No activity" |
| jobs running | green, breathing between about 45 % and full opacity over 2.5 s (a `SequentialAnimation`, not a timer) | "2 running" |
| a job failed and nobody has looked | red, larger, a white `!` — **outranks running**, and stays until the popup has been opened | "1 failed · 2 running" |

Green is a meaning, not a palette slot: a theme whose `green` is reddish gets `#34b354`. Machinery (below) never lights it. The "N running ·" words stay in the bar's status text, beside it.

## The popup

Opened by the orb, 400 px wide, as tall as it needs up to the window, anchored above the orb with a point aimed at it (the point slides along the bottom edge and never into a corner). Header "Activity", with **Clear** at the right when anything has finished. Entries newest first; **an entry keeps its place when it finishes**. "No activity" when empty. It closes on `Esc`, on a click anywhere outside it, and on the orb; a click on the orb within 250 ms of an outside-click close counts as that close, not a reopen. It rebuilds only while it is up; hidden, only the model and the orb move.

**Which jobs**: everything the user asked for. Not the machinery — an undo's inverse ops (`_silent`), `movePairs`, `rmdirIfEmpty`, `chmodList`, a mirror's preflight scan — which the daemon marks `hidden`. And one-step jobs (`delete`, `chmod`, `trash`, `restore`, `mkdir`, `rename`, `emptyTrash`) are shown while they run and if they fail, and are gone once they finish well: a list of "Deleted 1 item" is noise beside the transfer somebody is waiting on.

**An entry**: the icon Icon view draws (folder or file, 32 px); a bold headline — the thing itself, "site", "a.txt and 2 more", "Mirroring local → remote", not a sentence about it; then

- *while it runs*: a 5 px bar — with no length while it is queued, preparing or cancelling — and a dim status line. Under a transfer of several files or a mirror, a ▶ opens a row for **the file in hand**: its name, its own 4 px bar, "12 KB of 254 KB (45 KB/sec)". The ▶ is absent while the job is still preparing.
- *once it is over*: a wrapping line saying what came of it; a failure in the danger colour, ending "— see Log".
- buttons, round, at the right: **Log** (always); **Cancel** while it is unfinished; **Reveal** for a finished job that made something on this machine (opens its folder with it selected — kiki is the file manager); **Dismiss** otherwise.

| Case | Status line |
|---|---|
| queued | Waiting… |
| preparing (a transfer walks the whole tree before it knows its totals) | Preparing to transfer… |
| one file | `1.2 MB of 42.7 MB (350 KB/sec)`, bar over bytes |
| several files | `12 of 458 transferred — 2% complete`, bar over **files** — a bar of bytes stalls on the big one |
| mirror | `120 of 900 items · 40.2 MB of 1.10 GB · 2.1 MB/sec` |
| delete, chmod, trash… | `Processing 12 of 458 items` |
| cancelling | headline becomes "Cancelling…", the bar loses its length, the line clears — said the moment Cancel is pressed |

| Ended | Completion line |
|---|---|
| download / upload | Downloaded 458 items / Uploaded 1 item |
| local | Copied 3 items / Moved 3 items |
| mirror | 412 copied, 9 deleted, 3 skipped (skipped only when there were any) |
| delete, chmod | Deleted 3 items / Changed permissions on 3 items |
| cancelled | Cancelled / Mirror cancelled |
| failed | the error, without the names of what passed it along ("Io: russh::Error: Connection reset" → "Connection reset"), the daemon's bare codes as words ("report.pdf: not found"), "Failed" when there is nothing |

Sizes: bytes under 1 KB, whole KB, MB to one decimal, GB to two.

## The job log

**Log** opens a window over kiki's: the job's log, newest at the bottom, following the tail until somebody scrolls away. Times are from the job's first line. kiki's own lines stand forward; the libraries' sit a little back; warnings are yellow and errors in the danger colour. A filter box (text and source, any case; `Esc` clears it, then closes), **Copy all**, and a note of how many earlier lines were let go. The sidebar's location menu opens the same window on a **location's connection log** — for a location that will not connect there is no job to look under.

What is in it:

- **kiki's own account**: what was asked and from where to where; each file as it is begun, with its size; cancel requested; how it ended, with counts; a failure with the file's name.
- **What was done on the server**, the same for every plugin, from the SDK: `connect` and `disconnect` (with the session's role), `write path (n bytes)`, `read`, `mkdir`, `delete`, `rename a -> b`, and any refusal as a warning with its code. Listing and stat are left out — a browser does thousands.
- **What the library says** through the `log` facade. Measured on real transfers before deciding anything: suppaftp's `Debug` is a story ("Put file …", "PASV command", "Renaming … to …"); russh's is the wire ("> msg type 94, len 128" — 380 lines for six small files). So `russh`, `russh_sftp`, `rustls`, `tokio` and `mio` are heard from `Info` up, where their warnings and errors are; everything else at `Debug` while a job's session is open and `Info` otherwise; never `Trace`.

**Whose line**: the SDK stamps each with the role of the session its thread is serving. A job's sessions are its own (`job-<id>`; plan 31 phase 2), so that is whose line it is. A line from one of a library's background threads carries no role and goes to every job with a session open on that plugin at that moment. The browser's lines belong to no job and live in the plugin's log.

**Secrets are taken out inside the plugin**, before a line leaves the process: what follows the FTP `PASS` command, and what follows `password`, `passphrase`, `authorization`, `secret` or `token` *when a `:` or `=` comes next*. A sentence that merely mentions one — "password authentication failed for gideon" — is left whole: it is the line somebody needs. A log is something people paste into bug reports.

**Forwarding is asked for** (`KIKI_PLUGIN_LOG`, set by the daemon when it spawns a plugin). `Log` events arrive between other frames, a binary stream's included; a host that reads frames strictly and did not ask gets none.

**Kept**: a ring per job and per plugin — 2,000 lines or 256 KB — that says how many earlier lines it let go. A job's log goes when the job does. A **failed** job's log is appended to `~/.local/state/kiki/failed-jobs.log` (begun again when it would pass 1 MB), so last night's failure is still there in the morning.

## Protocol additions

`Job` gains, beside `id, op, state, done, total, bytes, bytesTotal, title, error, undoable`:

| Field | Meaning |
|---|---|
| `name`, `count`, `isDir` | the thing the job is about (the first of them), how many, and whether it is a folder — for something on a server, known once the transfer has listed it |
| `src`, `dest`, `direction: "upload" \| "download" \| "remote" \| "local"` | |
| `phase: "preparing" \| "running"` | derived: running with no totals and no bytes yet |
| `cancelling: bool` | derived: cancel asked for, not yet ended. `Cancel` broadcasts a `JobEvent` at once |
| `current: { name, bytes, size } \| null` | the file in hand, while running. `size` 0 when its own progress is not tracked |
| `rate: u64` | bytes per second, smoothed by the daemon so every window shows the same number; 0 when not running |
| `result: { copies, deletes, skipped } \| null` | a mirror's |
| `revealUri: Uri \| null` | something on this machine the job made |
| `hidden: bool` | machinery |

`done`/`total` count **files** for a copy or a transfer (a folder copy used to end at "1 of 458"), items for a mirror as they finish, and a job that ends `done` ends at its totals. Totals are announced when they are known (`set_totals` broadcasts).

| Request | Fields | Reply |
|---|---|---|
| `ClearJobs` | | `{ cleared }` — every finished job is forgotten; live ones never |
| `DismissJob` | `job` | `{ cleared }` |
| `JobLog` | `job`, `from?` | `{ lines: [{ t, level, source, text }], next, dropped }`; ask again from `next` |
| `LocationLog` | `location`, `from?` | the same, for the location's plugin |

Event `JobsCleared { jobs: [id] }`. All four requests are answered in `server.rs` and the event is broadcast from `jobs.rs`; `kikid/tests/protocol.rs` scrapes every request type out of `server.rs`'s own match arms, so a request documented here and answered by nobody fails a test. `Jobs` returns every live job and the fifty most recent finished ones. Forgetting is the daemon's because the list is fetched again on every reconnect, which would bring back whatever a window had only hidden.

Plugin → daemon (only when `KIKI_PLUGIN_LOG` is set): `{ event: "Log", level, target, message, role }`.

**IPC**: `shell activityView open|close|toggle|clear|state` → `{ open, orb, tip, entries: [{ id, headline, state, line }] }`.

## Verification

- `tst_ActivityText`: which jobs are shown, every status and completion line, the orb's three states and what clears the red one, a long job outliving fifty short ones. `tst_ActivityOrb`: the orb's looks, the popup's open and close rules and the 250 ms guard, a row per job newest first, the disclosure, the right buttons and what each sends, Clear. `tst_JobLogWindow`: reading on from `next`, the filter, Copy all, `Esc`.
- `jobs::tests`, `joblog::tests`, the SDK's `liblog::tests` (redaction, both what goes and what must stay).
- e2e **`activity`**, in the running window against the running daemon: a folder copy listed by its name and ending "Copied 3 items"; a trash and its undo leaving no entry; a failure turning the orb red until the popup is opened, with the error naming the file; a job that made nothing offering nothing to undo; the failed job's log on disk; Clear surviving a fresh `Jobs`.
- e2e **`remote_transfers`**, against real SFTP and FTPS: an upload says it is one, of which file and what size; `preparing` then `running`; a rate; `cancelling` while still `running`; and the job's log — opens with what was asked, names each file, holds the part file written and renamed into place on that job's own session, under 150 lines, no password in it or in the location's log, reads on without repeating, gone when dismissed.
- By hand, once (plan 31 phase 10): watch a large upload through it; cancel it; open the log of a transfer refused by a real server.
