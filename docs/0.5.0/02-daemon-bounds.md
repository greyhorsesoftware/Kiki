# 02 — The daemon's last unbounded things

**Status:** planned, 2026-09-29. The audit of 2026-09-18 (`docs/audit-2026-09-18.md`, §2 and
§3) listed what in the daemon can grow or wait without limit; most was fixed that week, and
what is left is here, re-checked against the tree today. None of it has bitten a user. All of
it is the kind of thing that bites once, on a machine that cannot be looked at.

## Today

- **The writer channel to a client is unbounded.** `server.rs:78` — `mpsc::channel::<Value>()`.
  Every event the daemon produces for a window (job progress, listing patches, watch changes)
  goes into it; a window that stops reading — a hung compositor, a `qs` stuck on a frame — keeps
  the daemon's heap growing at the rate of its own progress reports until the socket is closed.
- **`Plugin::shutdown()` can wait forever.** `plugin.rs:455` sends `Shutdown` and then
  `c.wait()` with no kill and no deadline. The idle reaper calls it, so one plugin that ignores
  `Shutdown` — a stuck SSH session, a share helper mid-handshake — stops reaping for every
  plugin from then on. `describe_all()` calls it on the client thread, so a `Plugins` request can
  hang a window's connection the same way.
- **A plugin request has no ceiling.** `REQUEST_TIMEOUT` is 120 s (`plugin.rs:14`), but
  `wait_reply_with` resets on every frame: a plugin that says something every 119 s holds the
  thread for ever. Right for a slow transfer that is moving; wrong for one that is not.
- **`bsdtar` is waited on for as long as it says nothing.** `archive.rs` checks the cancel flag
  per output line; a `bsdtar` that produces no output — a pipe to a dead extractor, a FUSE mount
  that went away — is waited on with nothing checking.
- **The mirror's audit log is appended forever.** `mirror/execute.rs:38`, `audit.log` in the
  state dir; `access.rs` compacts its own log at 50 000 lines (`COMPACT_ABOVE`), this one does
  not.

## Decisions

1. **The writer channel gets a bound and a policy, not just a bound.** A `sync_channel` alone
   would block the daemon's worker on a slow window, which is the wrong end to punish. So: a
   bound of 4 096 frames; when it is full, *progress* events (`JobProgress`, listing stat
   patches — anything a later one supersedes) are dropped oldest-first, and anything else
   (replies, `JobDone`, errors) is delivered by closing the connection if there is no room in a
   second — a window that far behind has to reconnect anyway, and reconnecting resubscribes.
   The count of dropped frames goes to `kikid.log` once per connection, not per frame.
2. **`shutdown()` has a deadline and a kill.** `Shutdown` sent, 2 s for the process to go,
   then `kill()` and `wait()`. The reaper and `describe_all` both get it for free. A plugin
   killed that way is logged with its name.
3. **A total ceiling on a plugin request, separate from the idle one.** `REQUEST_TIMEOUT` stays
   the between-frames patience; a new `REQUEST_CEILING` of one hour ends *any* request, moving
   or not, with a numbered error — an hour is longer than the longest transfer anyone has run
   through kiki by a factor of ten, and short enough that a stuck plugin does not outlive a
   working day. Transfers that legitimately need longer are the mirror's, which run per file.
4. **`bsdtar` gets the same shape:** a `wait_timeout` loop that checks the cancel flag every
   200 ms whether or not there is output, and a ceiling of an hour.
5. **`audit.log` compacts** the way `access.rs` does: at 50 000 lines the oldest half is
   dropped, atomically (`.tmp` and rename). The mirror's report does not read the log, so
   nothing else changes.

## Levels

- **L1** decisions 2 and 5 (an afternoon): the deadline in `shutdown()`, the compaction. Tests:
  a stub plugin that ignores `Shutdown` is gone within 3 s and the reaper goes on; an audit
  log written past the cap comes back shorter with its newest lines intact.
- **L2** decision 1 (a day): the bounded channel and the two policies. Tests: a client that
  subscribes and never reads sees the daemon's RSS flat over 10 000 progress events, then gets
  disconnected on the first non-progress frame; a client that reads slowly gets every
  `JobDone`.
- **L3** decisions 3 and 4 (half a day): the ceilings. Tests: a stub plugin that emits a frame
  every second for longer than the ceiling (the constant is an env override for the test)
  ends with the numbered error; a `bsdtar` stand-in that sleeps with no output is ended by
  cancel within a second.

Nothing on the wire changes but one new error number for a request ended by the ceiling
(`error.<n>` in the three catalogs, `API-DELTA.md`).

## Verification

`cargo test` (the new tests are unit tests in the modules they bound), the audit's table in §3
updated to read "present" on every row, and `kikid.log` inspected after a full e2e run for
the dropped-frame line, which must not appear — the harness's windows read promptly, so a
drop there is a bug in the bound, not in the window.
