# Activity View — specification

Build a transient "Activity" popover that shows everything the app's background job
manager is doing right now, plus a short history of what it just finished. It is the
single place a user goes to answer "is it still going, how far along, and did it work?"

## Entry point — the toolbar button

- A flat, icon-only toolbar button (~36×36) sits in the main window's toolbar, between
  the filter field and the Mirror button. Tooltip: "Activity".
- The button carries a small status badge in the lower-right of its icon (~10px):
  - **Jobs running** → a green orb (RGB ~52,179,84) that pulses its opacity smoothly
    between ~45% and 100% on a ~2.5s cycle (40ms timer, sine-driven alpha).
  - **Any job failed** → a red rounded warning triangle with a white exclamation mark;
    errors outrank the running orb.
  - **Nothing running, nothing failed** → no badge at all.
- Clicking toggles the popover. Guard against the reopen race: the popover hides on
  focus loss, so a click that arrives within 250ms of a focus-loss hide counts as
  "toggle off", not "reopen".

## The popover

- An undecorated, always-on-top, non-modal window with a transparent background,
  custom-painted as a **rounded rectangle (12px radius) with a speech-bubble arrow**
  (10px tall, 20px wide) pointing up at the toolbar button.
- Fixed width 400px; height grows to fit content (repack whenever content changes).
- Anchored under the button and horizontally centered on it, clamped to stay on screen;
  the arrow slides along the top edge to keep pointing at the button, but never closer
  than corner-radius + half-arrow to either corner.
- Colors come from the look-and-feel (panel background, component border color), so it
  follows light/dark themes.
- Dismisses automatically on window focus loss.
- Header row: centered bold "Activity" title, with a small rounded "Clear" button on the
  right that removes all *finished* entries (running/queued ones stay).
- Body: a vertically stacked, scrollable list of entries, newest at the top, separated by
  thin horizontal rules. When there is nothing to show, a centered dim "No activity"
  placeholder.

## What gets tracked

Only user-meaningful job types appear: **download, upload, delete, chmod, mirror**. All
other internal jobs (list, login, ping, refresh, disconnect…) are invisible here.

An entry is created when the job is enqueued (state Queued) or, if it wasn't seen in the
queue, when it starts. Entries keep their position in the list through their whole
lifecycle — they never re-sort when they finish.

States: **Queued → Active → (Completed | Failed | Cancelled)**.

Delete and chmod entries auto-remove themselves on success (no lingering noise); the rest
stay until the user clears or dismisses them.

## Anatomy of an entry

Three columns in a grid:

1. **Icon** (top-aligned, 32×32): a folder or file glyph depending on the job's target.
2. **Content column** (stacked):
   - Bold 12pt header: the file/folder name, or for mirrors the job name
     ("Mirroring local → remote").
   - *While running or queued:*
     - A thin (5px) progress bar — indeterminate until real numbers exist.
     - A dim 11pt status line, prefixed for transfers/mirrors by a small ▶/▼ disclosure
       triangle that expands a per-file detail section. The triangle is hidden during
       the "preparing" phase and appears once bytes start moving.
     - **Detail section** (collapsed by default, indented): current file name, a 4px
       per-file progress bar, and a 10pt line like "12 KB of 254 KB (45 KB/sec)".
       Expanding/collapsing repacks the popover.
   - *Once finished:* the bar and status are replaced by a dim, word-wrapping subtitle
     (see Completion text).
3. **Action buttons** (vertically centered, circular 36px buttons with a subtle circular
   hover wash):
   - **Log** — opens the connection/transfer log for that job in a log window.
   - **Cancel** (unfinished entries) — asks the job manager to stop the job.
   - **Reveal** (finished transfers with a known local path) — opens the containing
     folder in the OS file manager.
   - **Dismiss** (finished entries with no local path) — removes just that entry.

## Live progress

A single 1-second repeating timer polls every Active entry and rewrites its labels; it
starts when the first job starts and stops as soon as nothing is queued or active. The
view never pushes work onto the job threads — it only reads status snapshots.

Status-line formats by case:

- **Preparing** (job is enumerating, zero bytes transferred): indeterminate bar,
  "Preparing to transfer…", disclosure hidden.
- **Single-file transfer**: bar measured in KB against the file size; status
  `"1.2 MB of 42.7 MB (350 KB/sec)"`; no detail section.
- **Multi-file transfer**: bar measured in *files*; status
  `"12 of 458 transferred — 2% complete"`; detail section shows the current file's own
  name, bar, and byte/rate line. Per-file byte counters reset each file, so keep a
  cumulative total by detecting the reset (new value < last value) and banking the old one.
- **Mirror**: reads the mirror's live run state — bar over total actions, status
  `"120 of 900 items · 40.2 MB of 1.10 GB · 2.1 MB/sec"`, detail row shows the current
  relative path with a per-file bar for copies and the action name ("delete", "skip")
  otherwise.
- **Other item-count jobs** (delete, chmod): bar over item count, "Processing 12 of 458 items".
- **Cancelling**: header flips to "Cancelling…", bar goes indeterminate, status clears.

Sizes are humanized: raw bytes below 1 KB, then KB with no decimals, MB with one
decimal, GB with two.

## Completion text

- Transfers: "Downloaded 458 items" / "Uploaded 12 items".
- Delete/chmod: "Deleted 3 items" / "Changed permissions on 3 items".
- Mirror: "412 copied, 9 deleted, 3 skipped" (skipped only when non-zero).
- Cancelled: "Cancelled" / "Mirror cancelled".
- Failed: the underlying error's message with any leading fully-qualified error-type
  names stripped — repeatedly, since they nest (a raw message of the form
  "some.package.SomeError: other.package.OtherError: Network is unreachable" shows as
  "Network is unreachable") — falling back to "Failed" when there is no message.

## Behavioral rules

- The panel is a pure *observer*: it listens to queue events (job enqueued) and job
  manager events (started, key-job changed, completed, failed, cancelling, stopped,
  shutting down) and never drives the jobs except through explicit cancel.
- All UI rebuilds are cheap and idempotent: rebuild the list, repack, reposition,
  refresh the toolbar badge. Rebuild only when the popover is actually visible; a hidden
  popover just updates its model and the badge.
- When a child job fails, mark that job's own entry; only fall back to the parent/key
  job's entry if the child has none — otherwise the real entry stays "active" forever and
  the toolbar orb never stops pulsing.
- On app shutdown: stop the timer, clear all entries, hide the popover.
