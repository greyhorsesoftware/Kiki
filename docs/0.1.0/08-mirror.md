# 08 — Mirror

**Status:** built and tested, local and SFTP (one FTPS run where `vsftpd` is installed); Edit rules… built 2026-09-21.

Builds on: `04-operations-and-undo.md` (job queue), `06-remote-locations.md` (plugins, session cloning), `07-split-mode.md` (the two roots — now Side by Side).

Origin: the design is ported from RelaySFTP's shipped one-way mirror. Everything the port needs is in this file; the engine reference in the appendix is the authority where the prose above is a summary.

## Goal

A one-way, authoritative mirror between the two split panes: one side is the master, the other the replica. After a run the replica matches the master under the two roots. Preview is mandatory, deletes are opt-in and guarded, and a run is a cancellable queued job.

Not in scope: two-way sync, following symlinks, mirroring permissions.

Departures from the original implementation, all deliberate:
- kiki runs **N actions concurrently** (~~default 3, adjustable on the Running screen~~ **five, hard-coded, with no selector** — owner, 2026-09-21; the Running screen's label says what is used, and the daemon's default is the same five, clamped 1–8 for anything that asks. What five means: `execute` resolves each side once and every worker shares the job's one session, and a plugin process carries one binary stream at a time (FTPS serialises everything behind its session mutex) — so against a server the five overlap the mkdirs, deletes and set-times around one transfer, and it is local-to-local runs that go five files at once. A whole `mirror_sftp` run held at most two connections to `sshd` and one to `vsftpd`) instead of serially. Creates still complete parent-before-child (a copy waits for its parent's mkdir) and deletes still run child-before-parent, after all creates. The whole run still occupies one queue slot.
- Progress is **pushed** per action on `JobEvents`, not polled, and the Running screen is a table of every action with its status rather than a single bar. This is also what lets a run that finishes while the workspace is closed refresh both panes, closing a known gap in the original.
- The Mirror button is enabled whenever Side by Side is active with both panes connected, including a local-to-local pair, so the local end-to-end tests run through the same UI. **A folder is never mirrored into something inside it** — refused before the scan, in the workspace and in the daemon both (`mirror::scan::overlap`), so a client that skips the form is refused too.

## Engine (`kikid/mirror`)

Three phases, split so the destructive part is always previewable:

```
Spec ─▶ scan ─▶ master map + replica map ─▶ diff (pure) ─▶ Plan ─▶ execute ─▶ Result
```

**Spec**: `master, replica` (root URIs, resolved to backends by the daemon), `direction: Upload | Download`, `delete_extras = false`, `blast_radius = 0.5`, `confirmed_large_delete = false`, `clock_offset_ms = 0`, `clock_offset_auto = true`, `detector`, `modified_within: Option<Duration>`, `apply_filters = false`. `scan` writes the measured offset back into the spec and the UI reuses that instance for execute.

**Entry**: `rel_path` (`/`-separated, root stripped), `is_dir, size, mtime_ms (0 = unknown), digest: Option<String>`, plus the backend handle for execute. Symlinks are always skipped.

**Scan**: depth-first walk through `Backend::scan` on both sides, taking `Meta` inline where the backend provides it (SFTP and FTPS do, so the remote side costs one readdir per directory; with SFTP exec acceleration, plan 06, the whole remote tree arrives in one streamed command) and running an `Enrich` pass with relative `statx` on the local side. Subdirectories are walked by up to N workers in parallel where per-directory listing is the only option. Filters (filename predicates from `~/.config/kiki/filters.toml`: contains, starts with, ends with, matches) are applied symmetrically at scan, so a filtered replica file is never an "extra" and never deleted, and a filtered directory is skipped with its whole subtree. Scan is read-only and runs as a plan-04 job with no journal entry, so it is cancellable with `Cancel { id }` and its result is the plan.

**The default rules — nine names, each an `is`** (amended 2026-09-21; `mirror::filters::default_rules`): `.git`, `.gitignore`, `.DS_Store`, `.env`, `.idea`, `.vscode`, `Thumbs.db`, `node_modules`, `__pycache__`. They were first built as the one pattern `startsWith "."`, which silently kept `.htaccess`, `.htpasswd`, `.user.ini`, `.nojekyll` and everything under `.well-known/` — Let's Encrypt's challenge among it — from ever being uploaded by a website mirror, with nothing to show for it but "N filtered out". A dotted name is mirrored like any other now unless a rule names it; whoever wants every hidden file skipped adds `starts with .` in the dialog. No `[[rule]]` in the file at all means the defaults; `defaults = false` beside no rules means the user deleted every one of them and meant it.

**"Edit rules…"** on the Configure screen, beside the filters check — **built 2026-09-21** (it had been listed as post-0.1.0, D16). `ui/MirrorRulesDialog.qml`: one row per rule — the kind as a box that cycles `is → contains → starts with → ends with`, the name, an x — with **+ Add rule**, **Restore defaults**, Cancel and Done; nothing is written until Done, a row left empty is dropped, and a name with a `/` in it is refused because a rule matches a filename and never a path. There is no reorder. Daemon: `MirrorFilters` / `SetMirrorFilters`, checked in full before anything is written (the old `Filters`/`SetFilters` had no caller and no validation — an unknown kind was coerced and a bad value dropped, silently). **One list, and it is the daemon's**: both requests answer with `defaultRules` beside `rules`, so the dialog carries no copy of the defaults to drift. Saving over a plan on Review goes back to Configure. `tst_MirrorRulesDialog` (20), `tst_MirrorWorkspace`, `mirror_local.py`.

**Diff** (pure, unit-tested on synthetic maps):
- Refuse if `delete_extras` and master is empty and replica is not.
- Walk master keys ascending (parents before children): master-only dir → `Mkdir/New`; master-only file inside the window → `Copy/New`, outside it → `Skip/Equal` (not copied, never deleted); both dirs → `Skip/Equal`; both files → `Copy/Changed` if the detector says changed and inside the window, else `Skip/Equal`; type mismatch → a create for the master node plus, if deletes are on, a delete of the replica node. Because all creates run before all deletes, that create usually fails against the existing node, is counted as skipped, and the next run converges. Accepted, as in the original.
- If `delete_extras`, walk replica keys descending (children before parents): replica-only → `Delete/Extra` or `Rmdir/Extra`.
- Plan = creates ++ deletes ++ skips, with `replica_entry_count` and `filtered_count`; derived `delete_count`, `blast_radius_fraction` (0 when the replica is empty), `copy_bytes`.
- `diff` takes `now` as a parameter so the window is testable.
- The modified-within window gates copies only, never deletes; unknown mtime is always inside the window; comparison subtracts the clock offset.

**Detectors** (`fn is_changed(master, replica, spec) -> bool`, files only):
- `SizeMtime` (default): size differs → changed; either mtime unknown → unchanged; `|master.mtime − offset − replica.mtime| > 2000 ms` → changed (boundary inclusive).
- `SizeOnly`: for FTPS uploads, where the remote mtime cannot be set.
- `Digest`: selected when a plugin's `change_detector` asks for it. Before the diff, any local side computes an MD5 for files whose counterpart carries a usable digest and has the same size, so a local ↔ digest-bearing remote compares content without mtimes. The engine falls back to `SizeMtime` when a digest is missing on either side.
- Selector: the remote side's plugin answers `change_detector(direction)`: SFTP → SizeMtime either way; FTPS upload → SizeOnly, download → SizeMtime; a local-only mirror → SizeMtime. The UI offers Automatic plus manual override of the two shipped detectors.

**Clock offset** (auto): for every path on both sides that is a file, same size, both mtimes known, collect `master.mtime − replica.mtime`; fewer than three samples → 0; else the median. Manual override in whole hours ±24. A plugin whose detector is `Digest` disables it.

**Deletion guards**, all before the first write:
- Preview mandatory (UI); deletes opt-in (spec default); filtered files never extras (scan); empty-master refusal (diff).
- Blast radius: `blast_radius_fraction > spec.blast_radius` without `confirmed_large_delete` → refuse; the UI asks "This will delete N of M items (P%). Proceed?" before queueing, since the job runs unattended.
- All execute-time guards are skipped when the plan contains no deletes.
- Containment: every delete target's absolute path must be the replica root or under `root + separator`, and `rel_path` must not contain `..`.
- Per-action opt-out: unchecked rows are dropped from the plan that runs, keeping the original `replica_entry_count`.
- Audit: each delete, each skip and each mtime failure writes a line to `~/.local/state/kiki/audit.log`; each run writes a summary.

**Execute**: enforce guards; seed totals (non-skip count, copy bytes); run creates through a bounded pool of N workers that respects parent-before-child, then deletes the same way child-before-parent; per action: check cancellation; `Mkdir` → `mkdir`; `Copy` → stream through the daemon then best-effort `set_mtime` to the master's mtime (this is what keeps SizeMtime idempotent on the next run); `Delete`/`Rmdir` → `delete` + audit. A failed action is audited and counted as `skipped`, and the run continues (snapshot tolerance). Result: `copies, deletes, bytes, elapsed, skipped`.

**Job**: a mirror run is one plan-04 job carrying the spec and the confirmed plan. It clones fresh sessions from the location (plan 06) so it never shares the browser's channel. On cancel or completion it logs those sessions out cleanly even when the cancellation token is already set, otherwise a half-open server session can break the browser's next listing (the original's bug, and what plan 06's cancel test checks). Progress is pushed on `JobEvents` per action (state, bytes), which is what the Running table renders. The job has no journal entry: a mirror is not undoable; the Review screen is the safeguard. This is a deliberate exception to plan 04's rule and is stated in the UI.

## UI (Quickshell)

**Entry point** (**amended 2026-09-21**): ~~the **Mirror to \<location\>** button in the Mirror view's bar (`Ctrl+M`), whose chevron picks upload or download~~ — plan 24's bar above the panes is off (`MirrorBar.qml` is wired and hidden), and the way in is the **Mirror…** button in the middle of the toolbar, with `Ctrl+M`. Its icon is two stacked arrows, one each way — a transfer between two sides — and it stands over the line between the two panes, wherever the sidebar and the divider have put it. `Ctrl+M` from one pane enters Side by Side first. It replaces the two panes with a workspace; leaving restores them (a running job continues), and coming back puts it at Configure rather than on the last run's table.

Header on every screen: local path, remote host and path, direction arrows (upload = local → remote, the default; download = remote → local), locked after Configure. **The paths wrap** (owner, 2026-09-21): each sits in its own column, at most 340 px and narrowing with the window, and the header grows to hold them — the screen below gives up the room.

1. **Configure**: detector (Automatic / Size + date / Size only), delete extras (off), skip items matching filters (on, with **Edit rules…** beside it), only files modified within N hours / days / weeks (off; 7 days when on), **the clock offset row** — a "Set manually" check, a whole-hours box (−24…+24, out of range refused, dimmed while automatic) and a line saying which way the hours go, *"The destination's clock is 2 hours behind the source"*, because the offset is `master.mtime − replica.mtime` and a sign the wrong way about re-copies a whole tree or skips one (built 2026-09-21; there is no Time Offset… dialog). Then the live plan sentence, Cancel / **Preflight**. Short of room the gaps close first, then the PLAN box takes what is left and its type shrinks to fit, to no smaller than 12 px; it never runs under the footer.
2. **Preflight**: indeterminate bar, "Comparing ~/site…  12,400 items so far", runs scan on the daemon. ~~Back cancels with no side effects.~~ The way out says **Cancel** (owner, 2026-09-21: "I hit Back, then went back into the screen and it was still showing Comparing…"), Escape does the same there, and it leaves nothing behind — a Cancel pressed before the `Submit` reply lands stops the job the moment it exists. **And the compare itself now stops**: the daemon polled the cancellation token only when a frame failed to arrive, so a plugin streaming fast enough was never told and a scan of 50,000 entries was walked to the end; it gives up at 600 now, 5 ms after the ask. Connecting is cancellable too (732 ms, where it was 120 s), and the digest path gives up inside a file rather than reading it whole.
3. **Review**: tabs All / New / Changed / Unchanged / Delete; table with checkbox, operation (`↑ copy (new)`, `↑ copy (changed)`, `↑ mkdir`, `✕ delete`, `✕ rmdir`, `= unchanged`, arrow follows direction), path, size; footer "N copy · M delete · X MB to transfer · K filtered" (**the two copy counts are added, not joined** — it read "31 copy" for 3 new and 1 changed). **The footer is two rows**, one held to each edge with the summary eliding between them: as one row with a spacer it pushed "Save report…" and **Mirror** off the window at ordinary widths. Back, Cancel, Save report…, **Mirror**. Mirror builds the sub-plan from checked rows, shows the blast-radius confirmation if needed, then queues the job; ~~if it has no actions the workspace simply closes~~ **an empty sub-plan runs an empty job and says "Mirror complete · 0 copied"** (2026-09-21), which is an answer rather than a window vanishing.
4. **Running / Done**: overall bar with "done / total items · bytes · speed" and ~~the concurrency selector~~ a label saying how many run at once (five, fixed); a table of every action with its status (done, in-flight with a per-file bar, queued, skipped with the reason); on completion "Mirror complete · N copied · M deleted · X MB [· K skipped]", on failure "Mirror failed: …", on cancel "Mirror cancelled" — the Done summary used to say only "Mirror complete", and a cancelled run showed the advice given before one starts. Both panes re-list on completion and cancellation. Cancel while running, Close when done.

**Remote to remote**: when master and replica are served by different plugin processes, each file streams from the master's `Read` straight into the replica's `Write` through a bounded in-memory channel (no temp file, both sides busy at once). Two locations on the same plugin process spool through a temp file, because one plugin serves one binary stream at a time.

**Memory of the three collections** (from the plan-26 benchmark): the master map, the replica map and the plan each hold one entry per path, and the naive port carried an owned `String` per path in the map key, again inside each `Entry`, and again in each `Action`, so a 200,000-file scan held four copies of every path. The engine first moved to one `Arc<str>` per path shared by key, entry and action, then to a true arena: `SideMap` holds every relative path of one side in a single byte buffer with `u32` offsets and one `Entry` per path, sorted by path bytes after the scan (`finish`), so lookups are binary searches and parents still sort before children. The diff walks the master side in order and looks each path up on the replica side; there is no per-path allocation and no tree-map node. Actions keep their own path string (one `Arc<str>` each) because the Review screen, the report and the executor read it directly. `SideMap` implements `FromIterator<(impl AsRef<str>, Entry)>` and `Index<&str>` so the synthetic-map tests build it the same way they built the tree map.

**Report**: plain text built from spec and plan only: direction, both roots, detector, clock offset (with which way it goes — `7200000 ms (manual, subtracted from master mtime)`), delete extras, window, filters and how many were filtered, summary counts, then one line per action with the compared values on each side (size, mtime, digest). This is what makes churn and skew diagnosable.

**Save report…** on Review (2026-09-21): it opens kiki's own chooser in save mode with a name offered (`kiki-mirror-<host-path>.txt`), **the daemon writes the file** (`MirrorReport { job, saveTo }`) and a toast says where it went. Before that it wrote `~/kiki-mirror-report.txt` through a shell `printf`, silently. IPC: `mirror saveReport`, then `mirror save <uri>`.

**Protocol**: `Submit { MirrorScan { spec } } -> JobId` whose result is the plan, `Submit { MirrorRun { spec, plan } } -> JobId`, `MirrorReport { job, saveTo? } -> String`, and `MirrorFilters` / `SetMirrorFilters` for the rules (each answering `rules` and `defaultRules`). The plan can be large, so it is served like a listing: `MirrorPlan { job } -> { id, counts }` opens it as a windowed listing filterable by reason and the Review table scrolls it through a `WindowCache`; `MirrorCheck { id, rows, checked }` sets per-row opt-out in the daemon. The Running table is the same listing with per-action status patched by `JobEvents`. **IPC added**: `mirror(open|close)`, `mirrorScreen()`.

**Mockups**: `MirrorConfigure.dc.html`, `MirrorReview.dc.html`, `MirrorRunning.dc.html` in `docs/design/`, plus the mirror button in `SplitView.dc.html`. Preflight is the Configure frame with a progress bar in place of the form; Done is the Running table with the summary line. Neither has its own artboard.

## Protocol behaviour

| | SFTP | FTPS |
|---|---|---|
| Upload and download | yes | yes |
| Create dirs | yes | yes |
| Default detector | size+mtime | size-only up, size+mtime down |
| Preserve mtime on copy | best-effort | download only |
| Auto clock offset | yes | yes |

An object-store plugin would add a `Digest` detector, implicit directories and no mtime preservation; see the appendix.

## Build order inside this plan

1. Scan + diff with `SizeMtime`, read-only preview. Unit-test the diff.
2. Execute copies and mkdirs (deletes off).
3. Deletes behind the opt-in with all guards and the confirmation.
4. `SizeOnly`, the plugin selector, auto offset, modified-within window.
5. Toolbar entry point and the four screens; the queued job; audit lines.

## Verification

- Pure diff: new file → Copy; new dir → Mkdir; size change → Copy; mtime beyond tolerance → Copy; within tolerance → Skip; dirs → Skip; unknown mtime → size-only; extra with deletes off untouched; extra file → Delete, extra dir → Rmdir with deletes on; empty master refused; creates parent-first; deletes child-first; counts and bytes; window for new and changed; window never deletes; window applies offset; offset is the median of same-size pairs, 0 with no common files, 0 below three samples.
- Detectors: every branch of `SizeMtime` and `SizeOnly`, the `Digest` fallback path with a stub digest, and the selector per shipped plugin and direction.
- Local end-to-end on two temp folders: scan plans 3 files + 2 dirs; execute copies; a replica-only file survives an additive run; a second scan is empty; editing one file yields exactly one Changed copy.
- Guards end-to-end: deletes remove replica-only items; blast radius aborts unless confirmed; empty master refuses; containment refuses a target outside the root.
- SFTP idempotence against a local `sshd`: an upload with spread source mtimes is a no-op on the second run.
- UI: drive the four screens against `sshd` and `vsftpd`, assert destination state and the Done summary.
- Report: header fields, summary counts, per-file lines.

**As run** (2026-09-21): `tests/e2e/flows/mirror_local.py`, 83 checks in the default run — Configure's defaults, scan → plan → run → done, the destination tree entry for entry, the report, a second run with nothing to do, one file edited planning exactly one changed copy, a row unchecked and then not run, deletes off and on, the audit line per delete, and every guard refusing (blast radius declined and confirmed, an empty master, a filtered name never an extra, a folder mirrored into itself, no run without a plan). `mirror_sftp.py`, 27 checks on the phase-4 fixture — an upload of times spread over twelve years that a second run finds nothing to do, a run cancelled at 192 MB that leaves no part file and leaves the browser listing that folder, a compare cancelled over 20,000 files a side, and one FTPS run where `vsftpd` is installed. `tst_MirrorWorkspace` and `tst_MirrorRulesDialog` under them. The screens have never been clicked by a physical pointer (cage maps none) — that is on the phase-10 list, with "Save report…" writing the file and how a long run looks.

## Appendix — engine reference

Distilled from the RelaySFTP implementation this design was ported from, in kiki's terms. Where the prose above is a summary, this is the behaviour to implement.

### Diff

```
diff(master: Map<rel, Entry>, replica: Map<rel, Entry>, spec, now) -> Plan
  if spec.delete_extras and master.is_empty() and !replica.is_empty():
      return Err(Safety("master scan returned no entries; refusing to delete the entire replica"))

  creates = []; deletes = []; equals = []

  for rel in master.keys().sorted():                 # ascending: parents before children
      m = master[rel]; r = replica.get(rel)
      if r is None:                                  # master-only
          if m.is_dir:              creates.push(Mkdir/New  (m, None, 0))
          elif within(m):           creates.push(Copy/New   (m, None, m.size))
          else:                     equals.push (Skip/Equal (m, None, 0))     # outside window: not copied, never deleted
      elif m.is_dir and r.is_dir:   equals.push (Skip/Equal (m, r, 0))
      elif !m.is_dir and !r.is_dir:
          if detector.is_changed(m, r, spec) and within(m):
                                    creates.push(Copy/Changed (m, r, m.size))
          else:                     equals.push (Skip/Equal   (m, r, 0))
      else:                                          # file on one side, dir on the other
          if spec.delete_extras:
              deletes.push(r.is_dir ? Rmdir/Changed (m, r, 0) : Delete/Changed (m, r, 0))
          creates.push(m.is_dir ? Mkdir/Changed (m, None, 0) : Copy/Changed (m, None, m.size))

  if spec.delete_extras:
      for rel in replica.keys().sorted().rev():     # descending: children before parents
          if rel in master: continue
          r = replica[rel]
          deletes.push(r.is_dir ? Rmdir/Extra (None, r, 0) : Delete/Extra (None, r, 0))

  Plan { actions: creates ++ deletes ++ equals, replica_entry_count: replica.len(), filtered_count }

within(m) = spec.modified_within.is_none()
         or m.mtime_ms == 0
         or (m.mtime_ms - spec.clock_offset_ms) >= now - spec.modified_within
```

Sorting keys lexicographically gives the ordering guarantee because a parent's relative path is a strict prefix of its children's. Directories are never passed to the detector. Creates run before deletes, so a type mismatch converges on the second run (the first create fails against the old node and is counted as skipped).

### Detectors

```
SizeMtime.is_changed(m, r, spec):
    if m.size != r.size: return true
    if m.mtime_ms == 0 or r.mtime_ms == 0: return false        # unknown timestamp: trust size only
    return |(m.mtime_ms - spec.clock_offset_ms) - r.mtime_ms| > 2000   # inclusive boundary: 2000 is unchanged

SizeOnly.is_changed(m, r, _):
    return m.size != r.size

Digest.is_changed(m, r, spec):                                  # reserved for an object-store plugin
    if m.size != r.size: return true
    md = usable(m.digest); rd = usable(r.digest)                # 32 hex chars, no "-N" multipart suffix
    if md and rd: return md != rd (case-insensitive)
    if md or rd:
        local = md5 of the non-digest side's content, streamed  # only reached when sizes match
        if local is Some: return local != the digest
    return SizeMtime.is_changed(m, r, spec)
```

### Clock offset

```
auto_offset(master, replica) -> i64
    deltas = []
    for rel in master.keys() ∩ replica.keys():
        m, r = master[rel], replica[rel]
        if m.is_dir or r.is_dir or m.size != r.size or m.mtime_ms == 0 or r.mtime_ms == 0: continue
        deltas.push(m.mtime_ms - r.mtime_ms)
    if deltas.len() < 3: return 0                              # one coincidental pair must not explain away a real change
    return median(deltas)
```

Stored into `spec.clock_offset_ms` by scan when `clock_offset_auto`; subtracted from master mtimes by `SizeMtime` and by `within`. Manual override is whole hours, −24 to +24.

### Execute

```
execute(plan, spec, job):
    if plan.delete_count() > 0: enforce_guards(plan, spec)     # blast radius, containment; before any write
    job.set_totals(non_skip_count, copy_bytes)
    creates, deletes = partition(plan)
    run_pool(creates, N, parent_before_child)                  # kiki: N concurrent, dependency-ordered
    run_pool(deletes, N, child_before_parent)

    per action:
        if job.cancelled(): abort("mirror cancelled")
        match action:
            Mkdir        -> replica.mkdir(replica_path(rel))
            Copy         -> copy(action); copies += 1; bytes += action.bytes
            Delete|Rmdir -> replica.delete(action.replica); audit("mirror-delete", rel); deletes += 1
        on backend error:
            if job.cancelled(): rethrow
            audit("mirror-skip", rel, msg); skipped += 1       # snapshot tolerance: keep going
        job.progress(action, state)

    audit("MIRROR", location, copies, deletes, bytes, elapsed)
    Result { copies, deletes, bytes, elapsed, skipped }

copy(action):
    stream master.read(path) -> replica.write(replica_path(rel)) with progress
    then best-effort replica.set_mtime(replica_path, master.mtime_ms)
         on Unsupported/error: audit("mirror-mtime-skip", rel)  # keeps SizeMtime idempotent where supported

enforce_guards(plan, spec):
    if plan.blast_radius_fraction() > spec.blast_radius and !spec.confirmed_large_delete: Err(Safety)
    for each Delete|Rmdir: abs = replica_path(rel)
        if !(abs == replica_root or abs.starts_with(replica_root + sep)) or rel.contains(".."): Err(Safety)

replica_path(rel) = replica_root.trim_end(sep) + sep + rel.replace('/', sep)
```

Session teardown on cancel: log the job's cloned sessions out cleanly even though the cancellation token is set, then propagate the cancellation. Skipping this leaves a half-open server session that breaks the browser's next listing.

### Review screen rules

All actionable rows start checked; `Skip` rows have no checkbox. **Mirror** builds a sub-plan from checked non-skip rows, keeping the original `replica_entry_count` so the blast-radius fraction stays meaningful. An empty sub-plan closes the workspace. The confirmation reads "This will delete N of M items (P%). Proceed?" and declining returns to Review. Operation labels: copy (new), copy (changed), mkdir, delete, rmdir, unchanged; the copy arrow follows direction.

### Report

Built from spec and plan only, no UI state:

```
kiki mirror report
==================

Direction:         upload (local → homelab)
Master (source):   file:///home/david/Projects/greyhorse-site
Replica (dest):    sftp://homelab/srv/www/greyhorse
Detector:          size+mtime
Clock offset:      3600000 ms (auto, subtracted from master mtime)
Delete extras:     false
Modified within:   all files
Filters:           on (6 filtered)

Summary: 17 to copy, 1 to delete, 6 unchanged  (replica had 27 items)

action/reason | path | master | replica | bytes
--------------------------------------------------------------------------------
Copy/Changed | footer.js | master: 3990b @ 2026-09-12 14:02:13 | replica: 3821b @ 2026-09-01 09:01:00 | 3990b
Delete/Extra | old/banner-2024.png | master: — | replica: 90112b @ 2026-01-02 08:00:00
...
```

**Amended 2026-09-21** to what `mirror::report` prints: the action and reason are spelled as the engine's own words — `Copy/New`, `Copy/Changed`, `Mkdir/New`, `Delete/Extra`, `Rmdir/Extra` — not in capitals; and the filtered count is on the `Filters:` line, not in the summary's parenthesis.

The per-side values are what make a churn or clock-skew problem diagnosable.

### Notes for a future object-store plugin

An S3-style backend has no real directories (synthesise folder nodes from common prefixes; `mkdir` puts an empty `key/` object), cannot set mtime (`last_modified` is the PUT time), and returns a content hash in listings (the ETag, quotes stripped, MD5 for single-part objects on AWS and the common compatibles, not for multipart objects). Such a plugin returns `DetectorKind::Digest` from `change_detector`, and the engine disables auto clock offset for it. The modified-within window is weak there because it reflects upload time.

### Deliberate limitations carried over

Symlinks are always skipped. Filters match on filename only, not path globs. A type mismatch with deletes off produces a create that fails and is counted as skipped. Unix permissions are not mirrored.
