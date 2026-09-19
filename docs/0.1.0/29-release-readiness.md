# 29 — Release readiness: what stands between the tree and 0.1.0

Builds on: every plan. This one adds no feature. It lists what has code but has never been proven, what a build does not actually ship, and the housekeeping a release needs — and orders it.

## Where the tree stands (measured 2026-09-19, on Omarchy)

| Suite | Result |
|---|---|
| `cargo test` | 112 passed, 0 failed |
| `make test-qml` | 155 passed, 0 failed |
| `tests/e2e/run.sh` under `cage` | 85 passed, 0 failed, 1 skipped (`pointer_ops`) |

Every numbered plan has code behind it. What follows is what that sentence does not say.

## A. Side by Side and mirroring: built, never driven (plans 07, 08, 24)

The largest untested surface in the tree. The mirror engine has 12 Rust tests (`kikid/src/mirror/tests.rs`: diff, detectors, guards, a local end-to-end on two temp folders). Nothing above the engine has ever been exercised: there is no e2e flow for split or mirror among the fifteen in `tests/e2e/flows/`, no QML test of `MirrorBar` or `MirrorWorkspace`, and no one has used either by hand.

**By hand first** — an hour with two local folders, then a real remote, against the plans' own Verification lists:

- Plan 24 (as amended by section J: no mirror bar, no Swap): clicking a location shows Side by Side; `Ctrl+4` on a local folder opens the last location on the right (home when there is none); opening an `sftp://` URI from the breadcrumb pairs it with its local folder; a mirror run started from the toolbar button or `Ctrl+M` uploads from the local side.
- Plan 07: both panes open at their configured paths; a cross-pane copy shows progress and `Ctrl+Z` removes the copy on the destination; a cross-pane move and its undo leave both trees as they started; collapsing and re-expanding the split restores both pane paths and selections.
- Plan 08: the four screens (scan, plan, run, done) against a real remote; destination state matches the plan; the Done summary and the report (header fields, summary counts, per-file lines) are right; a second run is a no-op; deletes-on removes replica-only items; the blast-radius guard aborts unless confirmed; an empty master refuses; a target outside the root refuses; **cancel mid-run and confirm the browser session still lists** (plan 11's checklist item).

Write down what breaks; fix; then freeze it in tests:

- **`tests/e2e/flows/split.py`** — two local folders: open split, cross-pane copy + undo, cross-pane move + undo, collapse/expand restores paths and selections. No server needed.
- **`tests/e2e/flows/mirror_local.py`** — two local folders through the UI: scan → plan → run → done, assert the destination tree and the report, second run empty, deletes-on, the guards.
- **`tests/e2e/flows/mirror_sftp.py`** — the same against a throwaway `sshd` on a high port with a temp host key and `authorized_keys` (the harness already fakes `secret-tool`). `sshd` is installed here; gate the flow on `have("sshd")` like `cage`. Covers plan 08's "SFTP idempotence: an upload with spread source mtimes is a no-op on the second run".
- FTPS against `vsftpd` is plan 08's other UI target; `vsftpd` is not installed here. Decide: install it for the harness, or ship FTPS mirroring as hand-verified only and say so in the release notes.

## B. Devices and network locations: generated, not shipped (plans 17, 25)

- `kiki-plugin-mtp`, `kiki-plugin-afc` and `kiki-plugin-gio` build on this machine. `kiki-plugin-ptp` does not link: `libgphoto2` is not installed (`-lgphoto2`, `-lgphoto2_port`). `sudo pacman -S libgphoto2`, rebuild.
- None of the four has been run against hardware or a server. Each needs one real session: an Android phone (MTP), an iPhone (AFC), a camera (PTP), an SMB share and a WebDAV server through `gvfsd` (GIO). List, copy out, copy in, eject / unmount, hotplug while kiki is open.
- **A build does not ship them.** `plugin::LOCATION_KINDS` is `["ftps", "sftp"]` and the workspace's `default-members` omits all four, so the app cannot open these location kinds even when the plugin binary is present. **Decide per kind: in 0.1.0 or not.** A kind that ships is added to both lists, to the PKGBUILD's `depends` / `optdepends` with its C library, and to the release notes; a kind that does not is moved to "Out of scope for 0.1.0" in `CORE.md` so the plan and the build agree.

## C. The pointer path is not tested end to end (plan 28)

`pointer_ops` skips: `cage` does not map a `wlr_virtual_pointer_v1` to an output, so file operations are proven end to end by keyboard only. The QML suite does drive the mouse, but through Qt's test events, not a compositor. Options, in order of cost: a different headless compositor for that one flow (`sway` with `WLR_BACKENDS=headless` maps virtual pointers); or accept the skip for 0.1.0 and run the pointer half of the manual checklist below. Pick one and record it in `11-testing.md`.

## D. The manual checklist (plan 11) — never run

One pass, on Omarchy, before tagging:

- drag into Favorites and between split panes feels right
- "Show in folder" from Firefox and from Signal raises kiki on the current workspace
- Open and Save dialogs from a GTK app, a Qt app and an Electron app
- the first-run keyring consent prompt
- every theme, eyeballed once against the visual baselines
- Hyprland tiling and border with two kiki windows open
- cancel a mirror mid-run on a real remote (covered by A)

## E. Open questions to close (from `CORE.md`)

- Does every Omarchy install carry a Secret Service provider (gnome-keyring), or does the PKGBUILD depend on one explicitly? Check a fresh install; if not guaranteed, add the dependency.
- Does kiki's portal backend replace the GTK portal wholesale or sit beside it as an option? Decide, then make `packaging/` and the first-run dialog say the same thing.

## F. Loose ends seen while running the app (2026-09-19)

- `Inspector.qml`: "Binding loop detected for property `height`" on the preview box whenever an image preview is up (`imageHeight` ↔ `height`). Harmless to look at, noisy in the log, and a loop Qt may one day resolve differently.
- `Daemon.qml:60`: `TypeError: Property 'handleEvent' … is not a function`, seen once in a session's log. Find what dispatches to a handler that is not there.
- `Theme.qml` reads three files that do not exist on current Omarchy (`~/.config/omarchy/current/theme/alacritty.toml`, `~/.config/gtk-{3,4}.0/settings.ini`) and warns for each on every start. The fallbacks are right; the warnings should go (check existence first, or read quietly).
- Portal registration: "Could not register app ID: Connection already associated with an application ID" on every start.
- **Column view does not show git status.** List rows carry the state letter after the name (`ListRow.qml`) and icon cells a badge (`IconPane.qml`); `ColumnsPane.qml` draws neither, so a modified or untracked file looks clean there. Each column is the same listing cache the other views read, so the rows should already carry `git` and only the column row's drawing be missing — confirm that first, for the first column and the ones opened from it. Show the same mark as the list, in every column, and cover it in `tst_ColumnsPane.qml`.
- **A location that could not connect looked like an empty folder** (fixed 2026-09-19). No view read the listing's error, and the reason was overwritten anyway by the "no listing" the Window request sent along with a failed Open comes back with. Now the pane says "Could not connect", gives the daemon's reason and a **Try again** (`PaneError.qml`, over both panes; `tst_PaneError.qml`). Found with a location whose host name does not resolve.
- **Side by side, a click in a pane did not give it the focus** (fixed 2026-09-19) — only its header did, and `Tab`. Nothing to do with the connection. `PaneFocus` hung from a holder of no size at the top of each pane's Column, and in the running app a press outside the holder's own 0×0 was never offered to the handler inside it; `tst_PaneFocus.qml` passed because it lays the overlay straight over a pane, without the holder. It now lies over the view, as the view's sibling inside its Loader. Proven with real clicks injected into the live window (`wlrctl pointer`): a file, white space, and the dead remote pane's error panel each take the focus. **That is also the answer to section C's gap for a Hyprland session**: `wlrctl pointer move/click` works where cage's virtual pointer does not — worth a `tests/live/` script for pointer-only behaviour.
- The info panel's video player: sound is on, there is no scrub bar, and it plays local files only. Decide whether any of the three changes for 0.1.0.

## G. Housekeeping

- `README.md` → Status is wrong twice: the QML has long run under Quickshell, and plan 17 is generated. Rewrite it from section "Where the tree stands".
- `CORE.md` → "Decisions already taken" still says plan 17 "is specified but not generated"; plan 17's own status line says generated. Make them agree (and see B for where the kinds end up).
- Give every plan a one-line **Status** under its title, as 17, 22 and 25 already have: *built and tested* / *built, unverified: what is missing* / *not in 0.1.0*. Today "are we done?" cannot be answered from the docs.
- Commit. The tree is 55 files ahead of the last commit, including the deletion of `kikid/src/listing.rs`. Split it by topic so the history can be read.
- Then: bump nothing (the workspace is already 0.1.0), write the release notes, `cd packaging && makepkg -f` on a clean checkout, install the package, and run section D against the **installed** kiki rather than the checkout.

## H. Large transfers: local → local, remote → local, local → remote

Every copy the suites make today is a handful of small files. Nothing has moved a LARGE folder — many thousands of entries, deep nesting, a few multi-gigabyte files — in any direction, and that is where a job queue, progress reporting, cancellation and memory use actually get tested. `kikid bench` times `copy_mb_s` and an `upload`, but a benchmark is one timed pass, not a correctness test.

**Fixtures** — reuse the generator plan 26 already has (`kikid bench gen`), with a transfer-shaped profile: ~50,000 small files across ~2,000 directories eight levels deep, plus two or three files of 1–4 GB, plus the awkward cases (empty directories, a symlink, a name with a newline and one with non-UTF-8 bytes, a read-only file, a zero-byte file, mtimes spread across years). Generated into a temp directory, never committed.

**Three flows**, each asserting the same things — the filesystem is the oracle (plan 28):

- `tests/e2e/flows/transfer_local.py` — local → local, same filesystem and across filesystems (`cross_fs.py` already knows how to find a second one).
- `tests/e2e/flows/transfer_download.py` — remote → local, from the throwaway `sshd` of section A.
- `tests/e2e/flows/transfer_upload.py` — local → remote, to the same server.

What each asserts:

- **The tree arrives whole**: same entry count, same sizes, same content (hash the large files; sample-hash the small ones), mtimes within the plugin's tolerance, modes where the destination can hold them, the symlink still a symlink.
- **Progress is honest**: totals are known before bytes move or are revised upward, never downward; bytes-done is monotonic; the job ends at 100 % and says Done; item counts and byte counts both move.
- **Cancel mid-run** leaves no half-written file under its final name, reports what did and did not arrive, and leaves the daemon and (for the remote flows) the browser session usable.
- **A failure in the middle** (the read-only file; a destination that fills; the server dropped with `kill`) fails that item, carries on or stops as the plan says, and the job's error names the file.
- **Undo** of a large local copy removes everything it created and nothing else.
- **The UI stays a UI**: with the job running, a listing of another folder still answers inside its budget, and the shell's frame time stays under the plan 11 budget. Record the daemon's peak RSS — a transfer must not hold the tree in memory.
- **Throughput** is recorded (not asserted) next to the plan 26 baselines, so a regression shows up as a number.

These are slow — minutes, not seconds. Like `gallery_perf`, they stay out of the default `tests/e2e/run.sh` and run by name (`--flow transfer_local`), and in CI on release branches only.

## I. The activity orb

A large transfer runs for minutes, and today the only way to see it is invisible: an unmarked 200 px click area at the right of the bottom bar that toggles `ActivityPopover`, beside a "3 running ·" prefix in the status text. Nothing says it can be clicked, and nothing at all shows when a job has failed.

**The orb** sits in the lower right-hand corner of the window, at the right end of the bottom bar, and is the one thing that says what the background is doing:

- **Jobs running** → a green orb, pulsing its opacity smoothly between about 45 % and 100 % on a ~2.5 s cycle (a QML `SequentialAnimation` on `opacity`, not a hand-driven timer).
- **Any job failed** → a red orb with a white exclamation mark. A failure outranks running, and stays until the popup has been opened — a failure nobody saw is still news.
- **Nothing running, nothing failed** → a dim, still orb. Always there, so the way in is always in the same place; a control that comes and goes is one you cannot learn.
- A tooltip says the same in words: "2 running", "1 failed", "No activity".
- It lives in the window, not in the bar's row of key hints, so it is still there in the modes that change or hide that row (project mode's narrow tree, gallery).

**Clicked, it opens the popup**: every job — running, queued, and the recent ones that finished or failed — each with its title, state, items done of total, bytes done of total, a progress bar while it runs, the error when it failed, and Cancel while it can be cancelled. `ActivityPopover.qml` already draws most of this; what it needs:

- anchored above the orb, its corner pointing at it, rather than floating at the window's bottom right with nothing to point to;
- queued jobs shown as queued rather than left out, and a failed job's error in full rather than only a red title;
- closes on a click outside and on `Esc`; a click on the orb within 250 ms of that close counts as the close, not a reopen (the focus-loss race the old spec calls out);
- a line to clear finished jobs.

The 200 px click area goes; the "N running ·" text in the status stays, as words beside the orb.

`docs/0.1.0/activity-view-spec.md` is an untracked file written for a toolbar button in another toolkit's vocabulary. Its badge states and its reopen guard are kept above; its placement is superseded by this section. Fold what survives into a plan document of its own in `docs/0.1.0/` and delete the file, so the tree holds one description of the feature.

- Section H's flows are its test bed: the orb pulses while a transfer runs, turns red when the read-only file fails and stays red until the popup is opened, and settles on Done; the popup lists the job with the same totals the flow reads from the daemon; Cancel in the popup is the cancel the flow asserts on.
- `tests/qml/tst_ActivityOrb.qml`: the three states from a fake `Jobs.list`, failed outranks running, the red state clears once the popup has been opened, a click toggles the popup, `Esc` and an outside click close it, the 250 ms reopen guard, and every job in the list has a row showing its state.

## J. "Mirror" view becomes "Side by Side", and stops touching view preferences

**Status (2026-09-19): built**, except where a bullet below says otherwise. `win.sideBySide` is a window property; each pane has a view of its own and `Pane.rememberViews` turns view memory off for reading and writing while split; the toolbar toggle, the draggable divider (`sideBySideRatio`), a path over each pane, a press anywhere in a pane focusing it (focus following the pointer was tried and removed the same day: reaching across a pane for the toolbar changed what the toolbar was about to act on), the mirror button over the divider, and "the focused pane stays" are in. Covered by `tst_PaneViewMemory`, `tst_SideBySideChrome`, `tst_PaneFocus`, `tst_BreadcrumbScroll` and the e2e `side_by_side` flow (37 checks, including `views.toml` byte-identical across a session). **Dropped, by decision (2026-09-19), and not to be tracked as issues:** a home for Swap, "last mirrored" and the mirror options (the strip that held them is off, and stays off); the `Ctrl+Alt+←/→` divider nudge; the git branch chip on a pane's header. **Not built:** rewriting old `view = "mirror"` entries in `views.toml` on load (they are read as "no view", which is harmless, but never cleaned out); the rename in the README and plans 07, 23 and 24. (Settings → General no longer offers "Mirror" as a default view: side by side is a layout, not a view a folder opens in. An old `default = "mirror"` still starts the window side by side and shows as List there.)

Two things are called Mirror today: the two-pane view (`Ctrl+4`, the view menu's "Mirror") and the one-way copy it can run ("Mirror to homelab", `Ctrl+M`, the mirror bar, the workspace, the report). The first is a layout; only the second mirrors anything. The layout is renamed.

**The rename** — user-facing strings only, for the view and nothing else:

- Keymap `viewMirror` label "Mirror view" → "Side by Side" (the view menu entry goes altogether; see "Its own toolbar button" below); the shortcuts overlay's "other pane (mirror view)" and "icon / list / columns / mirror / gallery"; Settings → General's default-view choice; the README and plans 07, 23 and 24 where they mean the layout.
- **Not renamed**: "Mirror to …", "last mirrored", "Mirror complete", the mirror bar, `MirrorWorkspace`, the report, `Ctrl+M`, plan 08. That is the operation, and the word is right for it.
- The stored value `"mirror"` in `settings.toml [view] default` and in `views.toml` keeps being read (as "side by side" for the default; see below for `views.toml`), so nobody's config breaks.

**Side by Side ignores view preferences** (plan 21's per-folder memory):

- Entering it, and navigating either pane while in it, does **not** look the folder up in `views.toml`: a folder remembered as Gallery or Columns must not open that way in half a window. Both panes start in List.
- Click a side to focus it; the view menu then changes **that pane's** view (Icon, List, Columns, Gallery) — and the other pane keeps its own.
- That change is **not written** as the folder's preference, and neither is a sort or hidden-files change made while in Side by Side. Leave Side by Side and the folder opens the way it was last remembered from a single pane, as if the detour never happened.
- The view menu shows the focused pane's view, and nothing about the layout: that has its own control.

**Its own toolbar button.** Icon, List, Columns and Gallery are ways of drawing one folder; Side by Side is how many folders the window shows. They are different kinds of thing, and a menu that lists them together says they are the same — which is how the layout came to be stored in `Pane.view` in the first place.

- A toggle button in the toolbar beside the view button, using the `split` icon that is already in `icons.js`; lit while Side by Side is on; tooltip "Side by Side (Ctrl+4)". `Ctrl+4` keeps working and toggles the same state.
- "Mirror" leaves the view menu, which goes back to four entries. `Ctrl+1/2/3/5` still set the focused pane's view.
- In a narrow window the toolbar already drops buttons by width (`ViewSwitcher` hides under 360 px); this one follows the same rule and `Ctrl+4` remains.
- This reverses two earlier decisions and their documents must say so: plan 23's "one view button with a menu" (still true — for views) and plan 24's Verification line "the toolbar has no split or mirror buttons". Amend both.

**Each pane carries its own path; the title bar's steps aside.** (Built 2026-09-19. An earlier draft of this section pinned the title-bar path to the local pane and gave only the other pane a path of its own; tried, and replaced by this the same day: two paths in two different places for two equal panes read as one of them mattering more.)

- Side by side, each pane's header holds a real breadcrumb over its own listing — crumbs that navigate, a path you can type, the folders above on a right click — for the local pane exactly as for the remote one. Using it focuses that pane and moves only that pane.
- The title bar's path is hidden while the two are up. It keeps its room (invisible and disabled, not removed), so the toolbar's buttons do not slide about when the layout is toggled. With one pane it is back, and is that pane's path.
- `Ctrl+L` and the path menu go to the breadcrumb that is on screen for the focused pane: the title bar's with one pane, that pane's header side by side (`win.activeCrumb()`).
- The strip that used to sit above the panes (`MirrorBar`) is off. The way into a mirror run is a button in the toolbar, standing over the line between the panes (it follows the divider, kept inside the room the hidden path leaves) — the line with an arrow pointing each way — and `Ctrl+M`, which leads the bottom bar's key hints while side by side. Swap, "last mirrored" and the mirror options went with the strip; that is accepted, not an open issue.

**Turning it off keeps the pane you were in.** Two folders are showing and there is room for one. Today the left pane always wins: `leaveMirror` (`Shell.qml:57`) only changes the left pane's view back, the right pane stops being drawn, and focus is never handed back — `win.pane` can be left pointing at the hidden pane, so keys and the toolbar may act on something nobody can see. And the right pane is usually where the work was: `openLocation` puts local on the left, the remote on the right, and focuses the right. Three folders deep into a remote, `Ctrl+4` for room, and you are back at the local folder with your place gone.

- The **focused** pane is the one that remains. Focus left: as today. Focus right: the single pane shows the right pane's folder, with its selection, scroll position and history.
- Focus lands on the pane that remains, so `win.pane` is always something on screen.
- The folder that goes away is kept, not dropped: turning Side by Side on again brings it back on the other side, where it was. `enterMirror` already reuses the right pane's folder when it has one; the same must hold when the *left* one was the one put away.
- The single pane then opens that folder the way plan 21 remembers it — the preference, not whatever view it had while split (see "ignores view preferences" above).
- Done by swapping which `Pane` object the single layout draws, not by copying a URI from one pane into the other: a copy loses the selection, the scroll position and the back/forward history, and re-lists a remote folder for nothing.
- As built: the two `Pane` objects change places in `win.left` / `win.right`, and the view is rebuilt for the pane it now shows rather than re-pointed (its rows and caches were made for the other one). Folder, selection and history come across; **the scroll position does not** — the rebuilt view starts from the selection. Worth a second look if it is noticeable.

**The divider drags.** Today the two panes are hard-wired to half the window each (`Shell.qml:796` and `:815`, `Math.floor((parent.width - 1) / 2)`) with a 1 px line between them. A local tree beside a remote one rarely wants equal room.

- The line between the panes becomes a grip: a 6 px hit area over the 1 px line, `Qt.SplitHCursor`, accent-tinted under the pointer, `preventStealing` so a list underneath cannot take the drag — the same grip the info panel already has (`inspector-grip`), so the two feel alike.
- It sets a **ratio**, not a width, so resizing the window keeps the proportion. Each pane keeps a minimum (about 280 px — enough for a name column and the pane header); the ratio is clamped so neither side can be dragged shut. Collapsing is what the toolbar button is for.
- Double-click the grip: back to half and half.
- The ratio is remembered as one window-level setting (`settings.toml [view] sideBySideRatio`), written when the drag **ends**, not on every pixel — like `inspectorWidth`. It is not per folder: it describes the window, and per-folder memory is exactly what this section takes out of Side by Side.
- The mirror bar and each pane's header and filter follow their pane's width; a pane header too narrow for its breadcrumb elides the breadcrumb rather than pushing the other pane.

**Why this is more than a guard in `_remember()`**: today Side by Side *is* a view — `win.split` is defined as `left.view === "mirror"` (`Shell.qml:37`). So the left pane cannot have a view of its own while split (its `view` is taken), and because `Pane.onViewChanged` calls `_remember()`, merely entering it writes `view: "mirror"` into `views.toml` as that folder's preference — the folder then reopens split from anywhere. The layout has to come out of `Pane.view`:

- `win.sideBySide` becomes a window property of its own; `Pane.view` goes back to meaning only how one pane draws (icon / list / columns / gallery). `toggleMirrorView`, `leaveMirror`, the location-opens-split path (`Shell.qml:43–67`) and the `Loader` that picks each pane's view component all move onto it.
- `Pane` gains `rememberViews: true`; the window sets it false on both panes while `sideBySide`. When false, `open()` skips the `viewPref` lookup and `_remember()` returns early — one switch for read and write, so the two cannot drift apart.
- **Migration**: an entry in `views.toml` with `view = "mirror"` is an artefact of the old design, not a choice anyone made. On load, drop the `view` from such entries (keep their sort and hidden).
- "Remember view per folder" off (plan 21) already means no reads or writes; Side by Side behaves as if it were off, whatever the setting says.

Tests:

- `tests/qml/tst_SideBySide.qml`: each pane's header is a breadcrumb that navigates that pane and only that pane, the title bar's path is hidden while they are up and back with one pane, and the toolbar's buttons do not move when it goes (`tst_SideBySideChrome.qml` and the e2e `side_by_side` flow — both written); `Ctrl+L` edits the focused pane's own breadcrumb; the mirror button sits at the end of the local pane's header and follows it across a Swap; toggling off with focus on the right leaves the right pane's folder on screen with its selection and history, and `win.pane` is that pane; toggling off with focus on the left leaves the left; toggling on again brings the other folder back on its own side without re-listing it; after toggling off, a key (say `Del`) acts on the visible pane and never on the hidden one; the toolbar button toggles the layout and is lit while it is on, and `Ctrl+4` does the same; the view menu has four entries and none of them is the layout; dragging the grip changes both panes' widths and their sum stays the window's; the ratio survives a window resize; neither pane goes under its minimum however far the grip is dragged; a double-click returns to half and half; `sideBySideRatio` is written once, on release, not during the drag; entering does not call `SetViewPref`; a folder remembered as Gallery opens as List on either side; the view menu changes only the focused pane; no view, sort or hidden change made inside writes a preference (assert on `Wire.count("SetViewPref")`); leaving restores the single pane's remembered view; a stored `view = "mirror"` entry is dropped on load; `[view] default = "mirror"` still starts split.
- `tst_ViewSwitcher.qml`: four entries, the focused pane's view checked.
- The e2e `split` flow of section A is written against the new names and asserts `views.toml` is byte-identical before and after a Side by Side session.

## K. Drag and drop between the two panes, spring-loaded folders, modifiers

**What is there** (so this section starts from the code, not from nothing): rows in List and Icon view are drag sources (`Drag.Automatic`, `text/uri-list`, copy and move offered, move proposed), and folders, the pane background and sidebar favourites are drop targets. `Pane.qml:184–192` decides the action: **move within a scheme, copy across one, `Ctrl` forces copy**, a drop onto itself or into its own subtree is refused. `tst_DragDrop.qml` covers that policy with a synthetic drop; the pointer half has never run end to end (section C). **What is not there:** Columns and Gallery are neither sources nor targets; nothing has been dragged between the two panes of Side by Side by hand; there is no way to open a folder mid-drag; `Shift` means nothing; and "copy across a scheme" is the whole of the remote story.

### Between the two panes

- Side by Side is what drag and drop is *for*: drag from one pane, drop on the other — onto a folder row to put it in that folder, onto white space to put it in the folder the pane is showing. Every view is both a source and a target, Columns and Gallery included (a column is a folder; dropping on a column's white space drops into that column's folder).
- The drop makes a **job** (`CORE.md`: every write is a job), so progress, cancel, collisions and undo are the ones the rest of the app already has, and the activity orb (section I) lights for it.
- A press that becomes a drag must not also be the click that focuses the other pane and must not disturb the selection being dragged (section J's press-to-focus: focus moves on the *drop*, to the pane that received it).
- Dragging out of the window (to a terminal, a browser upload) and in from outside keep working as they do now; this section must not regress them.

### Spring-loaded folders

Hold a drag over a folder and it opens, so the drop can go somewhere that was not on screen when the drag began.

- **Hover → flash → open.** The folder under the pointer highlights at once (it is a drop target already). Held still for ~700 ms it flashes twice (~120 ms each) — the warning that it is about to open — then the target view navigates into it. Moving off before the flash ends cancels; the delay is a setting (`[view] springDelayMs`, 0 = off).
- It chains: inside the opened folder, hold over another folder and it opens too. The breadcrumb crumbs and the sidebar's favourites and locations are spring targets as well — hold over a crumb to go *up*, over a location to open it — so any destination can be reached without letting go.
- **Dropping puts the target view back where it started.** The view that sprang open is a detour, not a navigation: on drop — and on cancel (`Esc`, or a drop on nothing) — the target pane returns to the folder it was showing when the drag began, with its selection and scroll position, and the detour leaves no entries in back/forward history. The job's toast names where the files went, since that folder is no longer on screen.
- The **source** view never moves during a drag, even when it is the same pane: in a single pane, spring-loading navigates the pane you are dragging from, which is fine — the drag carries URIs, not rows — and it too returns on drop.
- Only folders spring. An archive, a location that is not connected (would need a password prompt mid-drag) and the trash do not; they are plain drop targets or not targets at all.
- A remote folder springs too, and listing it takes time: show the pane's loading state, and do not flash-and-open a second level until the first has listed.

### Modifiers: copy, move, link

One rule, the same in every view and for every target, shown **while dragging** — the cursor badge and a one-line hint by the pointer ("Copy to homelab:/srv/site") change the moment a modifier goes down or the target changes, so nobody finds out what happened from the result:

| Held | Action |
|---|---|
| nothing | the default for this source → target pair (below) |
| `Ctrl` | copy |
| `Shift` | move |
| `Ctrl+Shift` | link (symlink; local → local only, otherwise not offered) |
| `Alt` | drop, then ask: a small menu at the pointer — Copy here / Move here / Link here / Cancel |

- Modifiers are read at **drop** time and continuously during the drag, not captured at the start: people press them late.
- A right-button drag is `Alt`: it always asks. (Whether the compositor delivers right-button drags through `Drag.Automatic` is to be checked, not assumed.)
- `Esc` cancels the drag, returns any sprung view, and changes nothing.

### Local and remote

The default action depends on what the two ends are, because "move" between machines is a copy followed by a delete of the original, and a half-finished one loses nothing only if it is done in that order:

| From → to | Default | Notes |
|---|---|---|
| local → local, same filesystem | move | a rename; instant |
| local → local, other filesystem | move | copy then delete; a job with progress (`cross_fs.py` already finds a second filesystem) |
| local → remote | copy | upload. `Shift` moves: upload, **verify**, then delete the local original |
| remote → local | copy | download. `Shift` moves: download, verify, then delete the remote original |
| remote → same remote | move | a server-side rename where the plugin can (`Features`), else copy + delete through this machine |
| remote → other remote | copy | streamed through this machine; there is no server-to-server path. `Shift` moves, and says it will take as long as the copy |
| anything → trash | move to trash | local only; a remote has no trash (`CORE.md`: remote delete is a confirmed, non-undoable delete) — dropping remote files on the trash asks, in danger colours |
| trash → anywhere | restore/move out | |
| anything → archive, device, read-only location | refused, or copy where the target can take it | the target's plugin `Features` decide; a refused target shows the "no" cursor *during* the drag, not an error after it |

- A move whose delete half fails (permissions, the link dropped) is reported as "copied; the original could not be removed", never as a failure that implies nothing arrived — and never retried into deleting something that did not copy.
- **Undo**: a local move or copy undoes as it does now. `CORE.md` already rules that remote delete is not undoable, so a remote *move* is undoable only until its delete half runs; the job says which half it is in. A copy to a remote undoes by deleting what it created, behind a confirmation.
- Collisions use the existing prompt (`CollisionPrompt`); across a remote the comparison is by size and mtime within the plugin's tolerance, as mirror does.
- Dragging many small files to a remote is exactly section H's upload case; drag and drop adds nothing to the transfer itself and must not grow a second code path for it — `Ops.transferTo` is the one door.

### Tests

- `tst_DragDrop.qml` grows the table above: each source → target pair's default, each modifier overriding it, link offered only local → local, the trash and refused targets, modifiers read at drop time.
- `tst_SpringLoad.qml`: hold → flash → open after the delay and not before; moving off cancels; chained opens; drop returns the target to its starting folder with selection, scroll and history untouched; `Esc` does the same; delay 0 turns it off; a disconnected location and an archive do not spring.
- e2e `drag_between_panes`: needs a compositor that delivers pointer events (section C decides which); until then the IPC hook pattern used for the divider (`sideBySide drag …`) can drive a drag's *decisions* — begin, hover target, modifiers, drop — without a real pointer, and asserts on the filesystem.
- By hand, once: drag a folder of a few thousand files local → remote with the orb up, cancel it halfway, and check both ends.

## L. Signing in to an SFTP location (built 2026-09-19)

The form took one key file path and treated the password as "(if no key)"; there was no key discovery and no keyboard-interactive, so a server that asks for the password through PAM could not be signed in to at all.

- **Keys**: the form lists every private key in `~/.ssh` (found by content, not by name; the usual names first, in ssh's order), each with its type, comment and whether it needs a passphrase, and a tick beside each. A new location starts with the **first one ticked** and the rest are the user's to choose; only ticked keys are offered to the server, and none ticked is a password-only location (an earlier "any key found" choice was removed the same day: which keys to use is chosen in the form, not found by the plugin behind the user's back). The list is asked of the plugin afresh each time the form opens (`Browse`), so a key made a minute ago is there. A saved location that names a key since deleted still shows it, so it can be unticked.
- **Password** is an equal, not a last resort: a server that takes only a password needs nothing else filled in. It is tried after the keys, first as the `password` method and then as the answer to a **keyboard-interactive** prompt.
- At most four keys are offered, so sshd's `MaxAuthTries` (6) is not used up before the password gets its turn.
- A refusal says what was offered and what could not be: "the server refused 2 keys (id_ed25519, id_rsa) and the password — could not use work: needs its passphrase".
- **Two tabs, Password | Key**, with the Username above both (a key needs a username as much as a password does). Password holds the password; Key holds the key list and the passphrase. Only the chosen tab's fields are saved and sent, the choice travels as `auth`, and the plugin honours it: the Password tab offers no key, the Key tab sends no password (a location from before the tabs has no `auth` and gets both). A new location starts on Key when a key was found and on Password when none was. Tabs are a general form feature — any plugin field can carry a `group` (`sdk::field_in`).
- **Port shares Host's line** in every location form (SFTP and FTPS): a `port` field directly after another field takes 96 px at its right.
- **Local path**: its folder icon opens kiki's own folder chooser (the one it gives other apps through the portal, `PortalDialog.pick`), starting where the field points. The remote path's icon does not — a local chooser cannot answer for a server.
- **Pages: Connection | Locations** (SFTP and FTPS). The remote and local path are a page of their own; everything about getting in is on the first. Pages are sections, not alternatives: whichever is showing, the whole location is validated and sent, an error on the page you are not looking at marks its tab and takes you there, and the strip is fixed above the fields so it stays put while a page scrolls. A general form feature (`sdk::on_page`). Because a page is not a choice, the Password | Key choice is drawn differently — a segmented control, not a second row of underlined tabs.
- **The kinds stand down the left-hand side**, centred, instead of across the top: the band they took is the form's now. The card is wider (660) and shorter (560) for it.
- The dialog: **"Add" and "Add and Connect"**, centred as a pair ("Save" / "Save and Connect" when editing); **Add takes what was typed**: it checks that the required fields are there, keeps the secrets and writes the location — no name lookup, no sign-in, so a location can be added for a server that is off, or away from its network. **Add and Connect** signs in first (details and the server's key), then opens the location, and the choice survives the trust-this-server step. A location added unchecked has no pinned key, so its first connect pins the key if `known_hosts` vouches for it and otherwise refuses ("this server's key has not been verified yet") and offers the same trust step (`locations::key_verdict`, `Shell.offerVerification`). `Enter` is Add and Connect. It was one button called Connect, which said nothing about the location being kept — and did not in fact open it. No Cancel (the way out is the close box and `Esc`); what is happening or went wrong appears under it, full width; and no standing note about the keyring. Looked at in the running app, both kinds, both pages.
- **A location can wear a picture.** An IMAGE well at the right of Name opens kiki's own chooser (images only); the picture shows in the well, a cross on hover takes it off, and it is saved with the location as `image` (a path — the file is not copied, so a picture later moved or deleted falls back to the server glyph). In the sidebar, right-click → **Set Image… / Change Image… / Remove Image** does the same without opening the form or touching the connection (`SetLocationImage`, which changes the entry in place so it keeps its place in the list). Covered by `locations::image_tests` and `tst_LocationImage.qml`.
- For scripts and tests: `shell locationForm kind|page|auth <name>` over IPC.
- Covered by `key_tests` (discovery, labels, choice, messages, form) and seven new cases in `tests/mock_sftp.rs` against an in-process SSH server: several named keys tried in order, no key named offers none even when one would work, named-key-only (asserting the un-named key was never offered), passphrase needed / wrong / right, password after refused keys, keyboard-interactive-only server, nothing to sign in with, and `Browse`; and `tst_LocationKeys.qml` for the form. The tests never see the developer's own `~/.ssh` (`KIKI_SSH_DIR`).
- **Not done:** `ssh-agent` (keys held only by an agent are not found); `~/.ssh/config` (`IdentityFile`, `Host` aliases, `ProxyJump`) is not read; FTPS was not touched; and none of it has been tried by hand against a real server.

## Order

1. **G, first two bullets and the commit** — cheap, and everything after it is easier to review on a clean tree.
2. **J** — before anyone drives Side by Side by hand: it changes what the view is called, where its control is, how the panes are sized, how it is entered and what it writes, and section A's tests should be written once, against the structure that ships.
3. **A by hand** — the biggest unknown; it decides how much work is left.
4. **A's fixes and tests** — this builds the throwaway `sshd` fixture that H needs.
5. **I** — the orb first, so that H is watched through the thing users will watch it through.
6. **H** — large transfers, three directions.
7. **K** — drag and drop between the panes, modifiers, local/remote defaults, then spring-loading. After H because a drag to a remote *is* an H transfer, and after I because the orb is how a dragged transfer is watched. Needs section C's answer for its end-to-end half.
8. **B** — install `libgphoto2`, decide the kinds, one hardware session each for the kinds that ship.
9. **C** — decide; implement if it is the compositor swap.
10. **F** — the column view's git status first (a wrong answer on screen, where the rest are log noise), then the others.
11. **E**, then **D** against the installed package.
12. **G's per-plan status lines**, release notes, tag.

## Verification

- `make test` is green with `split`, `mirror_local` and (where `sshd` exists) `mirror_sftp` among the e2e flows, and `pointer_ops` either running or skipped for the reason recorded in `11-testing.md`.
- In Side by Side each pane shows and drives its own path in its own header, and the title bar's path is out of the way.
- Turning Side by Side off leaves the focused pane's folder on screen with its selection, focus on it, and the other folder waiting for the next toggle.
- Side by Side is a toolbar button of its own and is absent from the view menu; its divider drags, holds its ratio through a window resize, and comes back after a restart.
- The view is called Side by Side everywhere a user can read it, the operation is still called Mirror, and a Side by Side session leaves `views.toml` byte-identical.
- Every view is a drag source and a drop target; the default action and each modifier match section K's tables for every local/remote pair, shown while dragging; a folder held over flashes and opens, and the target view is back where it started after the drop.
- `tests/e2e/run.sh --flow transfer_local`, `transfer_download` and `transfer_upload` pass on the large fixture, cancel and mid-run failure included, with throughput and peak RSS recorded beside the plan 26 baselines.
- The activity orb shows running, failed and idle correctly through all three transfer flows, its popup lists every job with its state, and `tst_ActivityOrb` is green.
- Every location kind in `plugin::LOCATION_KINDS` has been opened against real hardware or a real server once, and every kind not in it is listed under "Out of scope for 0.1.0".
- A session's shell log is free of the four warnings in F.
- Every plan in `docs/0.1.0/` carries a Status line, and none reads "unverified".
- Section D has been run against the installed package and each line is ticked or has an issue number beside it.
