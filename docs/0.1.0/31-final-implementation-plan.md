# 31 — Final implementation plan for 0.1.0

**Status:** plan, written 2026-09-19. Nothing here is built yet. Supersedes the "Order" section of `29-release-readiness.md`; where this document and an older plan disagree, this one wins and the older plan is amended in phase 9.

Builds on: every plan. Inputs: `29-release-readiness.md`, `30-code-health.md`, `activity-view-spec.md`, and a read-only audit of every plan's Verification list against the tree (2026-09-19; five passes, nothing built or run). Findings the audit reached by reading code and did not run are marked **(probable)** — reproduce before fixing.

## What the audit changed

Plan 29 opens with "every numbered plan has code behind it". That is not the whole truth:

- **Plan 13's code viewer was deleted** (`CodeTab.qml`, commit 800883b); only the daemon half and the nvim bridge remain. **Plan 16 (project mode) is a prototype** with four probable defects, one of which may leave no keyboard way out of the mode. **Plan 12 (search)** works but none of its Verification bullets has been measured, and its scope UI is wired only into the portal dialog.
- **Plan 29 is stale in two places.** Section H: remote copy/move/delete/rename now exist (`kikid/src/transfer.rs`) and `remote_transfers.py` proves all nine end-pairs against a real `sshd` and a pyftpdlib FTPS server — which also settles section A's `vsftpd` question (pyftpdlib replaces it) and provides the `sshd` fixture. Section K: Columns and Gallery are already drop targets; List, Icon and Columns are drag sources.
- **Defects nobody had listed**: extract-undo can delete files that were there before; job sessions are never requested, so a transfer shares the browser's connection; a cancelled trash/delete ends as `done`; a second launch probably opens a second window; the package's install script sets root's folder handler; the share path leaks its temp directory on every error; a `git commit` in a terminal leaves badges stale.

## Decisions taken (owner, 2026-09-19)

1. **Location kinds**: `sftp`, `ftps`, and **gio** (`smb`, `dav`). `mtp`, `afc`, `ptp` are not in 0.1.0.
2. **Drag and drop ships** (plan 29 K) — between panes, modifiers, local/remote defaults. **Spring-loaded folders do not**; a later release.
3. **The activity popup carries the full entry anatomy** of `activity-view-spec.md` (it is how downloads and uploads are watched), placed as plan 29 I says: an orb at the bottom right, the popup anchored above it.
4. **The in-app Jarvis panel is removed.** "Open AI here…" and "Open Terminal here…" stay.
5. **Pointer testing**: `sway` headless was chosen for section C. The audit then found that `wlrctl` cannot hold a button down, so no headless compositor gives a real drag with the tools we have. Sway is kept as a time-boxed spike for *clicks*; drags are proven through an IPC drop hook (phase 5) and by hand.
6. **Everything in plan 29 F is fixed**, the video player included (reproduce the no-sound report first: the code has sound on, and the machine's default sink was HDMI).

## Defaults chosen here — each one is the owner's to overrule

These were "build it or strike it from the plan" choices the audit turned up. The rule applied: **fix what is wrong on screen or loses data; strike or defer what is merely promised.** Overruling one moves it from phase 9 (a doc edit) into the phase named.

| # | Item | Default | If overruled |
|---|---|---|---|
| D1 | Plan 13 code viewer (Code tab) | **decided (owner, 2026-09-19): kiki has no built-in code viewer — not deferred, not wanted.** Code is read in an editor, which is what Open in and the nvim bridge are for. The daemon half that outlived `CodeTab.qml` is removed in phase 1; plan 13 shrinks to the editor bridge | — |
| D2 | Plan 16 project mode | **fix the four defects, verify by hand, ship as it is**; everything else in the plan is post-0.1.0. If the hand pass is bad, hide the entry point instead | 3–3.5 days for the full plan |
| D3 | Plan 14 Open-in: Settings page, TOML watcher, `tab` placement, `follow`/`on_save`, `local_uri` fallback, malformed-entry report | post-0.1.0 | 1–1.5 days |
| D4 | Plan 20 Settings pages: Locations, Open in, Plugins (Devices is moot) | post-0.1.0 | 1–2 days |
| D5 | Plan 12 search as written (sorted index, streaming remote walk, scope menu, cancel, `IndexProgress`) | trim the plan to what is built; fix the two bugs; add tests | 5–7 days |
| D6 | Plan 27 gallery: Ctrl+wheel zoom, video poster, remote large preview, Filmstrip setting | post-0.1.0. **Fixed regardless**: `Del` advances, the image count, type-ahead off in gallery. **Decided (owner, 2026-09-19): a folder with no pictures is not an empty state** — Gallery shows its files and folders with the icons Icon view uses, scaled up (phase 2) | ~1.5 days |
| D7 | Plan 03 inspector: Created field, editable name, Git branch + last commit | Git detail **built** (the `GitStatus` request exists; ~2 h). **Decided (owner, 2026-09-19): the inspector's name is not editable** — renaming is done in the views (`F2`), and a second place to do it is a second thing to keep right; struck from plan 03, not deferred. Created field post-0.1.0 | Created: ~3 h (`statx` btime into `Meta`) |
| D8 | Plan 05 archives: "Extract here" folder rule, "Extract to…" chooser | **built** — extracting thirty files loose into the current folder is a wrong result, not a missing feature (half a day); `tar.bz2` added to the dialog | — |
| D9 | Plan 01 `Prefetch` (daemon has it, shell never sends it); mallopt A/B | strike Prefetch from the plan; record the mallopt numbers as they are and leave the calls in | half a day |
| D10 | `API-DAEMON.md` events never emitted: `Gone`, `FavoritesChanged` (the shell listens for both), `IndexProgress`, `SharePluginsChanged` | **emit the first two** (a second window's favourites go stale without it); delete the other two from the doc | — |
| D11 | `Hello` version check; `Read`/`Write` streaming text in the API doc | build the version check (1 h); delete the streaming text | — |
| D12 | D-Bus activation of `org.kiki.App` | drop `DBusActivatable=true` and the `.service` file | half a day to implement |
| D13 | Portal chooser over a remote location returns an `sftp://` URI | hide Locations in the chooser | half a day to fetch to a temp file |
| D14 | gio: `smb://` scheme handler + dialog prefill; "Encryption Required" (a field that does nothing); AFP | handler post-0.1.0; **remove the field**; AFP not shipped | half a day; 1–3 h; minutes |
| D15 | gio automated test against `smbd` | hand-verified only for 0.1.0, said so in its Status line | 1 day |
| D16 | Plan 08 "Edit rules…" filter editor | post-0.1.0 | half a day |
| D17 | Activity popup "Log" button | **decided (owner, 2026-09-19): built.** The audit said "the daemon has no per-job log", which is true and beside the point: the libraries underneath already write one and nobody is listening. `russh`, `russh-sftp`, `suppaftp` and `rustls` all log through the `log` facade, and no plugin installs a logger, so every line is dropped; `glib` has its own hook (`log_set_writer_func`). Capture it per job — phase 3 | — |
| D18 | Plan 24: a picture folder with no memory opens in **Gallery** (code) vs Icon (doc) | the code is right; amend the doc, add a test | — |
| D19 | Plan 23: `Backspace` is "parent folder" (code) vs "back" (doc and cheat sheet) | the code is right; fix doc and cheat sheet | — |
| D20 | Plan 11 visual-diff layer and shell performance probes; plan 26 shell half | post-0.1.0 — a suite that cannot be built in time must not hold the tag; the theme pass stays on the manual list | 3+ days |
| D21 | Remote copy is not undoable (`transfer.rs`, by decision there) vs plan 29 K's "undoes behind a confirmation" | keep the code; amend K | 1 day |
| D22 | Copy small files with parallel workers (plan 04) | post-0.1.0; phase 4's throughput numbers say whether it matters | — |
| D23 | `30-code-health.md` W6–W8 (Label/Rule, Shell's repeated handlers, Rust repeats) | 0.1.1 — wide, mechanical, and in conflict with everything in this plan | 1–1.5 days |

## Order

Eleven phases. Each ends with `make test` green and a commit (or several, by topic). Sizes are working days for one person and are the audit's estimates, not promises.

### Phase 0 — a clean tree (½ day) — **done 2026-09-19**

- The tree was committed whole (`7541e5d`), not split by topic; one local commit, left as it is.
- `Makefile` (30 W5): `test-qml` runs offscreen (`QT_QPA_PLATFORM ?= offscreen`, the environment wins); `clippy` is a target of its own and the first thing `make test` runs; `lint` is `clippy` plus the format check. `jobs::wait`'s poll stays in phase 2.
- Clippy at zero: the seven warnings fixed (two by hand — a `Chunk` type alias in the SFTP plugin, a doc list in LocalSend), and the empty `impl Inner {}` and `impl Listing {}` deleted.
- **The audit was wrong about FTPS**: `make e2e-servers` exists, `tests/e2e/.venv` is there, and the FTPS pairs of `remote_transfers` run on this machine. Nothing to do.
- **Baseline, 2026-09-19, clippy clean**: `cargo test` 145 passed, 0 failed; `make test-qml` (offscreen) 300 passed, 0 failed; `make test-e2e` under `cage` 183 passed, 0 failed, 1 skipped (`pointer_ops`, the virtual pointer). These replace plan 29's 112 / 155 / 85.

### Phase 1 — make the build match the decisions (1½ days) — **done 2026-09-19**

**As built.** Suites after it: clippy clean; `cargo test` 144 (two panel tests out, one in); `make test-qml` 306; `make test-e2e` 183 passed, 1 skipped; no error in the shell's log.

- **Jarvis panel**: gone as listed below — `ai.rs` is 203 lines of 408. One thing went further than planned: **a custom command is now run as written**, with `{prompt}` standing for the opening message and the word holding it left out when nothing is selected (`ai::custom_argv`, tested) — before, only its first word was run. Settings → Jarvis is **Settings → AI**, and its four texts describe a terminal, not a panel. `19-ai-query.md` is already the stub phase 9 asked for; `API-DAEMON.md` has `AiOpen` and `OpenTerminal` and has lost `AiQuery` and `AiCancel`.
- **Code viewer**: `kiki-plugin-highlight` is deleted with its sixteen grammars (249 lines out of `Cargo.lock`), and `OpenText`, `TextFind`, `text_window`, the per-client `texts` map, `helpers::highlight` and `editor.tabWidth` with it. `API-DAEMON.md` follows.
- **Devices**: `devices::openable` drops any device whose kind this build does not ship, and `devices::start` does not spawn its thread when none does; the sidebar already hid an empty section. Tested, with an assertion that reminds whoever ships a device kind to look here.
- **gio**: `LOCATION_KINDS` is `dav`, `ftps`, `sftp`, `smb`; `make build` makes the `kiki-plugin-smb` and `-dav` symlinks the package makes; the Encryption field is gone; both path fields are on a Locations page. Asked of a daemon built from the tree: SMB available; **WebDAV not — "install gvfs-dnssd"**, which is the unavailable path working: `LocationDialog` shows the plugin's reason in place of the form and will not add (`tst_LocationUnavailable.qml`). **Correction to the text below: on Arch the WebDAV backend is in `gvfs-dnssd`; there is no `gvfs-dav`.** The PKGBUILD's optdepends are `gvfs-smb`, `gvfs-dnssd`, `gvfs-wsdd`. Phase 8 needs `sudo pacman -S gvfs-dnssd` first.
- **CI**: differs from the text below on purpose. The device plugins stay in the tree, so one CI job still builds them (`-p kiki-plugin-mtp -p …-ptp -p …-afc`) and is the only one that installs their libraries — otherwise they rot unseen. The release workflow and the AUR `kiki-bin` package no longer name them; `kiki-bin`'s depends now match the main PKGBUILD. There is no macOS CI job, so nothing to exclude.

**As planned:**

**Jarvis panel out** (½ day). Delete `AiPanel.qml`; in `Shell.qml` the `aiOpen` / `aiQuery()` / `aiStatus` / `loadAi` members, IPC `aiQuery` and `aiClose`, the panel instance and its term in the width expression; in `server.rs` the `AiQuery` and `AiCancel` arms; in `ai.rs` `attachment`, `local_answer`, `query`, `run_cli`, `cli_children`, `cancel`, `cli_for` and their two tests (~230 of 408 lines), rebasing `status()` on `interactive_for`. Keep everything "Open AI here…" uses (`PROVIDERS`, `provider`, `interactive_for`, `opening_prompt`, `terminal_argv`, `open_external`, `AiStatus`, `AiConfigure`, `AiOpen`, `OpenTerminal`). **Settings → Jarvis**: keep status, provider and custom command; rewrite the four texts that describe a panel and print mode — and either honour `{prompt}`/`{files}` in a custom command or say that only the command itself is run. "ask Jarvis" → "open AI here" in `ShortcutsOverlay.qml:57` and `config.rs:204`; PKGBUILD's poppler line loses "and text for Jarvis". Design: `Jarvis.dc.html`, its `gen.py` block, `canvas.json`.

**Code viewer's remains out** (2–3 h; D1). `CodeTab.qml` went in 800883b; what it talked to is still built, shipped and called by nothing. Remove `plugins/kiki-plugin-highlight` (the directory, its `members` and `default-members` entries, `highlight` in the PKGBUILD's install loop) — with it go the sixteen tree-sitter grammars, their C builds and a slice of compile time and package size; `helpers::highlight()`; the `OpenText` and `TextFind` arms in `server.rs` and `open_text` behind them (`server.rs:187-200`, `841-865`), plus the `access::record` call made from `OpenText`; `editor.tabWidth` in `config.rs:119` and `Settings.qml:19` (never read). Check first that nothing else asks the highlight helper for anything — on 2026-09-19 the only callers were those two arms. The inspector's text and Markdown preview (`preview.rs`) is not part of this and stays. `API-DAEMON.md` loses the two requests; `API-PLUGIN.md`'s "service request sets are documented in plans 09, 13 and 19" becomes "plan 09".

**Devices off** (2 h). `devices::start()` runs in every build and the sidebar draws Devices, so a plugged-in phone shows and fails to open with `Unsupported`. Gate detection and the sidebar section on `plugin::ships`. Test: no Devices section when no device kind ships.

**gio in** (1 day).
- `LOCATION_KINDS` += `"smb"`, `"dav"` (both arms; never `"gio"` — kinds are the binary-name suffix). `default-members` += `plugins/kiki-plugin-gio`. PKGBUILD: install as `kiki-plugin-smb` with a `kiki-plugin-dav` symlink; `depends` += `glib2`, `gvfs`; `optdepends` `gvfs-smb`, `gvfs-dav` (**check** that Arch splits DAV out, and whether discovery wants `gvfs-wsdd`); `makedepends` += `glib2`.
- Dev and e2e runs point `KIKI_PLUGIN_DIR` at `target/release`, where only `kiki-plugin-gio` exists: the Makefile and `run.sh` create the two symlinks, or SMB and DAV are invisible to every test.
- `LocationDialog` honours `available` / `unavailableReason` (the plugin and SDK send them; nothing in `qml/` reads them) — the reason instead of the form, with a QML test.
- Remove the Encryption field (D14). Move the forms onto `on_page` / `field_in` so they match SFTP and FTPS (Connection | Locations; port on Host's line).
- CI and release workflows, and `packaging/aur/kiki-bin/PKGBUILD`: drop `libmtp`, `libgphoto2`, `libimobiledevice`, `usbmuxd` and the `-p kiki-plugin-{mtp,ptp,afc}` builds; add what the main PKGBUILD has (`libsecret`, `libarchive`, `udisks2`, `git`); macOS CI excludes the gio plugin.

### Phase 2 — defects (5 days) — **in progress**

**Done 2026-09-19: a job's own connection, and uploads that cannot be half files.** Suites after it: clippy clean; `cargo test` 144; QML 306; e2e 203 passed (20 new), 1 skipped.

- **Job sessions went deeper than the audit said.** It was not only that the daemon always asked for the `browse` role: the plugin protocol carried no role on a request at all, and both plugins looked every operation up under `browse` — so the `job` sessions they could open were never used by anything. And the FTPS plugin keeps a session behind one mutex which a transfer holds for its whole length, so **browsing an FTPS location froze for as long as any upload to it ran**. Now: every request names its session (`Session::req` is the only place a plugin request is started, so the role cannot be forgotten); the SDK hands it to the plugin as `sdk::current_role()`; SFTP and FTPS look their session up by it. A job that transfers, deletes, shares or mirrors enters `locations::JobSessions`, which gives it sessions of its own (`job-<id>`), opened only when it first touches a server and closed when it ends, however it ends. `mkdir` and `rename` are one round trip and stay on the browser's. A job's SFTP session is probed for fast scan like the browser's — a mirror scans too.
- **A cancelled upload was reported as done, and left a short file under the real name** — found by the new test, on SFTP and FTPS alike. A plugin is told a write is over by the bytes stopping and cannot tell "that was all" from "we stopped"; the daemon stopped sending on cancel **and on a local read error**, so either produced a truncated file and a successful job. Mirror uploads share the code. Uploads now go to `<name>.kiki-part` and take their name only when every byte has arrived (`mirror::execute::put`, used by all three upload pairings); anything else deletes the part and fails. SFTP's rename refuses an existing target, so one in the way is deleted first.
- **Downloads had the mirror image**: a failed local write (a full disk) was ignored and the short file renamed into place. Now an error. And the download's part name *replaced* the extension, so `notes.txt` and `notes.md` arriving on two mirror workers shared `notes.kiki-part`; it is appended now.
- **Tests**: `remote_transfers` — for each real server: when the jobs are over none of their connections is left; a 768 MB upload runs on a connection of its own (counted at the server's port); a folder never listed before lists while it runs, and the upload is confirmed still running at that moment; cancelled mid-file it ends `cancelled`, leaving neither a file nor a part file; the panes' connection survives; the job's closes. `plugin_contract` — a changed file replaces the replica's with no part left; an upload cancelled before its first byte leaves the replica's file exactly as it was.
- **Known and left**: a plugin process carries one binary stream at a time (`Plugin::binary`, the SDK's `stream_lock`), so while a transfer to a location runs, *previews and thumbnails* from that same kind of location wait; listings do not. Lifting it means a second plugin process for jobs — after 0.1.0 unless phase 4 shows it hurts.
- **Owed to phase 6**: `mirror_sftp` must upload a *changed* file, to prove the delete-then-rename fallback against a real `sshd` (the stub's rename overwrites).

**Done 2026-09-19, second pass: archives, half-finished jobs, share's fetch, search.** Suites after it: clippy clean; `cargo test` 150; QML 307; e2e 210 passed, 1 skipped.

- **Extracting never merges, so undo is exact** (D8 with it). `archive::extract` unpacks in a staging folder inside the destination and then takes ONE free name there: the archive's single item as itself, or — several items — a folder named after the archive (`site.tar.gz` → `site`, then `site (2)`). Nothing already there is merged into or overwritten, and the inverse is "delete that one thing". Before, an archive holding `src/` extracted beside an existing `src/` merged into it, and undo deleted the folder, the user's files with it. "Extract to…" now asks where (kiki's own folder chooser, through a new `Ops.folderNeeded`) instead of silently making a folder; the same rule decides what lands. The compress dialog offers `tar.bz2`.
- **The traversal guard is real and tested.** It read the member list through `list`, which stops at 10,000 entries — so entry 10,001 was never checked. It reads every name now (`bsdtar -tf`); the refusal is a typed `VfsError::Unsafe` (code `Unsafe`), and nothing is created, not even the destination. Tested with a real `..` archive, in Rust and end to end.
- **A job that stops half way can be undone.** `Job::undo_so_far` records the inverse as the work goes; the runner journals it when the job fails or is cancelled. Trash and local copy/move use it (a copy records its target *before* it is whole, so half a folder can be taken back; an undo's delete forgives a thing that was never made). And a job that left its loop early on cancel — trash, delete — now ends `cancelled`, not `done`. This is the local half of phase 4's "a cancelled big copy journals nothing".
- **Share fetches through the transfer code** instead of a copy of it: a remote folder arrives as a folder (it was `Read` as a file), with the job's progress and cancel, a full disk is an error, each item in a folder of its own, and the temp directory goes on every way out (a `Drop` guard). *Owed to phase 7:* a test of it, which needs the stub share plugin.
- **Search.** The index ranked only the first 40,000 matches it met, so the best hit — the exact name — could be one it never reached; it ranks every match now, holding the best 10,000 in a heap. `capped` is the index's own answer (exactly 10,000 was reported as "more"). A location whose plugin has no recursive scan — FTPS; SFTP without a shell — is walked folder by folder instead of finding nothing, and is capped like the index. Tested against both real servers. *Known and left (D5):* a location search still runs in the client's request thread, so that window's other requests wait for it.

**Done 2026-09-19, third pass: git status (plan 15).** Suites after it: clippy clean; `cargo test` 150; QML 316; e2e 217 passed, 1 skipped.

- **Column view** draws the list's letter, in the list's colours, in every column, and dims ignored rows. A column opened from another now takes the pane's daemon rather than the global one — no difference in the app, and what lets a test see an opened column at all.
- **One answer for every view**: `Format.gitMark(row)` and `Format.gitDimmed(row)` are what List, Icon and Columns all ask, so they cannot disagree, and they are where the settings act. **`[git] folders = "off"`** takes the mark off folders; **`showIgnored = "normal"`** stops the dimming (icon tiles dim too now; they did not). **`showIgnored = "hide"`** is the daemon's: ignored rows are left out of the listing the way dot-files are, and since a row is known to be ignored only once status has run, the view is rebuilt when it lands. It takes effect on the next listing of a folder, not retroactively.
- **git used in a terminal reaches kiki.** Nothing watched `.git`, so `git add`, a commit or a checkout left every badge and the branch chip stale. The watcher now holds two watches per repository being listed (`.git` and `.git/refs/heads`; at most 16 repositories, dropped once nobody lists under them), ignores git's `*.lock` files, debounces 300 ms, and re-runs status for every listing in memory under that root — each of which tells its windows `RepoChanged`. No listing is reset to do it. A repository whose status took over 2 s is skipped (`git::is_slow`, the flag that was set and never read), so a vast one is not re-run on every command. *Not watched:* `refs/remotes`, so ahead/behind after a `push` waits for the next change; branches with a `/` in their name.
- **Inspector**: state (with "· staged"), branch, and the last commit — short hash, author, relative date, subject — asked of `GitStatus` only for a row that is in a repository, and a late answer for the previous file is dropped.
- **Tests**: `tst_ColumnsPane` (three new), `tst_InspectorGit` (new), and the e2e flow **`git_status`** on a real repository: a state per row and a folder's aggregate; `git add`, `git commit` and `git checkout -b` from outside each arrive within the budget with a `RepoChanged` and no `Reset`; hidden ignored rows are absent.
- **For phase 9**: plan 15 loses the big-repository toast and gains the above; `gitState` IPC, the sidebar's dirty dot and the branch under a repository root in list view stay unbuilt — *not in 0.1.0* unless asked for.

**Done 2026-09-19, fourth pass: launch and packaging — and a sort that could hang.** Suites after it: clippy clean; `cargo test` 151; QML 317; e2e 223 passed, 1 skipped.

- **The launcher was broken twice over, confirmed by running it** (the audit had it as probable). It looked for a running kiki with `qs -c kiki`, a named config the package never installs — "Could not find kiki config directory" — so it never found one and every launch was another window. And it passed the folder as an argument after `--`, which the shell never reads (it reads `KIKI_START`), so every launch — every `xdg-open` of a folder — opened home. Now: the instance is addressed by the path of its `shell.qml`, the way it was started (`qs` exits 255 when there is none); a second launch calls the new `shell present <uri>`, which opens it and raises the window (`hyprctl dispatch focuswindow pid:…`); a first launch sets `KIKI_START` and runs `qs -n`. Paths, relative paths and URIs are all taken. A first launch also starts the user's `kiki.socket`, because the package can only enable it from the *next* login. e2e flow **`launcher`** runs the real script: the folder opens in the running window, and `qs list` shows no second instance.
- **The Neovim preset's callbacks went nowhere** for the same reason (`qs -c kiki ipc call shell saved`). They go through the launcher now: `kiki --ipc saved %:p`.
- **"Show in folder"** (FileManager1): the window comes to the front; the file is selected once the folder has listed, found by the daemon (it used to look only among the rows already loaded, so in a big folder nothing was selected); `ShowItemProperties` opens the info panel, which it ignored.
- **Packaging**: `DBusActivatable=true` and `org.kiki.App.service` are gone (D12 — nothing owns that name); `kiki-portals.conf` is gone (never read on Hyprland, and it said something the first-run dialog does not); `kiki.install` no longer runs `xdg-mime default` or `systemctl --user` as root, and no longer claims kiki is the folder handler — that is each user's choice, made in the first-run dialog. The desktop entry names SMB and WebDAV. The chooser shows no remote Locations (D13; `tst_PortalPick`).
- **Found on the way: sort by size or date could hang.** A full `cargo test` failed one run in three on a sort that never replied — not load, and not from this pass (the commit before it has the same code). A row that scrolls out of every window before its stat job runs is skipped by that job, and stayed marked `queued` for ever; `enrich_all` passes over queued rows, so the listing never finished enriching and a `Sort` waiting on it never got its answer. In the app: scroll fast through a big folder, sort by size, and the sort hangs. One line (`stats.rs`), a deterministic test that fails without it, and 40 runs of the suite under load without a failure where it had been 1 in 24.
- **For phase 10**, on the installed package: a second `kiki` raises the first; `xdg-open ~/Downloads` opens Downloads; straight after install, before logging out, `kiki` finds its daemon; "Show in folder" from Firefox raises and selects.

**Done 2026-09-19, fifth pass: plan 29 F, and the editor's socket.** Suites after it: clippy clean; `cargo test` 152; QML 330; e2e 223 passed, 1 skipped.

- **Theme no longer polls (30 W4).** How Omarchy switches was read off `omarchy-theme-set`: it deletes `current/theme` and moves a new folder into its place — so a watch on any file inside is dead after the first switch, which is what the 2 s timer was papering over — and then writes `theme.name` **in place**. That file keeps its inode, so its watch survives every switch; it is now the only thing watched, and its changing re-reads the palette, the icon theme and the fallbacks. Proven on the real `FileView` with a scratch file: three in-place writes, three change events. The timer is gone. The files that need not exist are read with `printErrors: false` (also proven quiet), and the alacritty fallback looks beside `colors.toml`, where current Omarchy keeps it, before the old `~/.config` path. `tst_ThemeReload`.
- **The portal warning is not ours.** "Could not register app ID: Connection already associated…" appears in a twelve-line Quickshell script with no kiki in it. Qt's or Quickshell's; recorded, and to be dropped from plan 29's Verification.
- **Inspector's binding loop**: an image's height was bound to the picture's implicit size while the picture filled the box. The ratio is now *set* when the picture has loaded. `tst_InspectorImage`. (The warning could not be made to appear offscreen, so that test asserts the sizing and the reset, not the absence of the warning — look for it in a live log in phase 10.)
- **Video player**: a strip under a playing video — mute, a scrub bar that seeks on press and drag, and `0:12 / 3:40` — shown under the pointer or while paused. Mute is the panel's, so the next video starts the way the last was left. New `volume` / `volume-off` icons (added to `icons.js` by hand: the file says it is generated from `gen.py`, but `gen.py` no longer has `play` or `pause` either). Local files only, as before. *Still owed:* the no-sound report itself, which needs a video and ears — the code has sound on, and the default sink was HDMI.
- **`jobs::wait`'s 5 ms poll** is called only by that module's tests: marked `#[cfg(test)]`, no condvar. `Daemon.qml` no longer throws if a listing forgets to unbind.
- **The editor's socket**: `$XDG_RUNTIME_DIR` is already 0700, so the audit's worry was the fallback — with no runtime directory the socket went into plain `/tmp`, where any local user could connect and drive the editor as you. It is a directory of our own there now, made 0700 and refused unless it is a real directory, ours, and closed to others (a symlink or an open directory under that name is refused). A finished session's socket file is cleared, so the next `--listen` does not fail with "address in use".

**Done 2026-09-19, sixth pass — phase 2 is complete.** Suites after it: clippy clean; `cargo test` 155; QML 340; e2e 229 passed, 1 skipped.

- **Gallery.** A folder with no pictures is a folder to look through: the stage draws the selected entry with the artwork Icon view uses (`UI.KindIcon`), scaled to about two fifths of the stage (128–320 px), its name under it, and the filmstrip uses the same artwork; an empty folder says "Empty folder". After a delete the stage moves to what followed (`keepPlace`) — it used to jump back to the first picture, so deleting through a shoot restarted from the top each time. Type-ahead is off in the gallery. The bottom bar reads "7 of 31" there. A double click on a folder in the filmstrip goes into it (it was handed to an external app). **Corrections to the text below, from reading the code:** the arrows already step through *every* entry, not only pictures, and stay that way (so the count is of entries, which is also all the shell can know — rows are windowed and only the daemon knows every kind); `←` on the first entry going up to the parent is deliberate, and kept; `selectFirst` already fell back to the first row.
- **Columns drag**: nothing to do — a column holds one selection, so the row under the pointer *is* the selection.
- **`views.toml`**: an entry with `view = "mirror"` loses its `view` on load and keeps its sort and hidden-files choice; the file is rewritten once and never again when there is nothing to clean (the byte-identical assertion in `side_by_side` still holds). The 1,000-folder cap now has a test that exceeds it.
- **Two windows.** `FavoritesChanged` is sent — to every window — when favourites are set; `LocationsChanged` went only to the window that made the change and is broadcast now; a client that names another protocol version in `Hello` is refused with code `Version` and a sentence it can show. **`Gone` needed more than sending:** a cached listing holds its folder open, and while anyone does the kernel reports nothing for the folder's own removal (measured: only the `DELETE` of a file inside it; `DELETE_SELF` arrives after the last holder lets go; an empty folder vanishes without an event at all). So the watcher asks each watched path once a second whether it still leads to the folder opened there (`still_at`, one `stat`, at most 64 of them). e2e flow **`two_windows`**, with two real clients.
- **Project mode (D2)**, all four, none of them run by hand yet: Neovim's preset takes a folder (`e` on a folder did nothing, and the error was dropped — a tool that will not start now says so in a toast); each tool's terminal has a class of its own (`kiki-tool-<id>`), a window is matched by pid first and never given to two roles, and kiki's own window is found by its pid rather than by a class it does not have; the tree takes the keyboard focus while it is up, and `Ctrl+Shift+P` leaves as well as `Esc`; asking again for a terminal tool that is already running *in the same folder* returns that one instead of starting a second agent. `tst_ProjectTree`, `tree::tests`. **Owed to phase 10:** one hand pass on Hyprland — Arrange cannot be tested without a compositor.



Things that are wrong today. Each gets a test that fails first.

**Data and correctness**
- **Extract-undo** (`jobs.rs:625-646`) deletes every top-level name it extracted — including a folder that existed before and was merged into. Record what extraction *created* and remove only that. With D8 (extract into a folder named after the archive when it has more than one top-level entry; "Extract to…" asks where).
- **Archive traversal**: the `../` guard exists (`archive.rs:68-73`) and has no test. A crafted zip and tar; a typed error code rather than `Io`.
- **Job-role sessions** (½–1 day). `locations::resolve` always connects as `browse` (`locations.rs:224`), so mirror runs and everything in `transfer.rs` share the browser's session; plans 06 and 08 promise a job its own. `mirror::side_for` and `transfer.rs` connect as `job`, and the session is dropped when the job ends, however it ends. Test: cancel an upload mid-file, then list — the listing answers.
- **Cancel**: local `trash` and `delete` break out of their loop and return `Ok`, so a cancelled one ends `done` (`jobs.rs:549-551`, `578-580`).
- **Share**: the `kiki-share-<job>` temp directory leaks on every early return; a remote folder is `Read` as if it were a file; the fetch reports no progress (`share.rs:148-201`).
- **Search**: `capped` is true at exactly 10,000 hits; the scan stops at 40,000 *before* ranking, so the best match can be missed; a plugin that answers `Unsupported` (FTPS; SFTP without exec) gets no fallback walk, so location search silently finds nothing.

**Git (plan 15)** (1 day)
- Column view draws the state letter and dims ignored rows — the rows already carry `git`; drawing only. `tst_ColumnsPane` case, first column and one opened from it.
- **Nothing watches `.git/index`, `HEAD` or `refs/`**, so `git add` or `git commit` from a terminal leaves badges and the branch chip stale (plan 15's third Verification bullet fails). Watch them per repository root; re-run status for the listed directories under it after the 300 ms debounce; emit `RepoChanged`.
- `showIgnored` and `folders` have Settings UI and no effect. Wire them.
- Inspector: branch and last commit through `GitStatus` (D7).
- Amend the plan: the big-repository toast is dropped — `git()` forces `core.fsmonitor=` off on purpose, so suggesting it contradicts the code; `Status.slow` is removed or read.

**Launch and packaging** (½ day; verified on the installed package in phase 10)
- `packaging/bin/kiki` probes with `qs -c kiki` and starts with `qs -p …`: the probe can never match **(probable)**, so every launch is a new window. Same `-c kiki` in the nvim preset (`openin.rs:31`). Use one addressing; focus the existing window on a second open.
- Drop `DBusActivatable=true` and `org.kiki.App.service` (D12).
- `kiki.install`: `xdg-mime default` and `systemctl --user` run as root — they set root's handler and print a false message. Remove; first-run Integrate is what does this, per user.
- Delete `/usr/share/xdg-desktop-portal/kiki-portals.conf` — xdg-desktop-portal never reads a file of that name on Hyprland; Integrate's per-user `FileChooser=kiki;gtk` is the switch.
- Portal: hide Locations in the chooser (D13); `showItems` honours `properties` (turn the inspector on); raise the window on `ShowItems`.

**Plan 29 F**
- Inspector's `imageHeight` ↔ `height` binding loop (`Inspector.qml:112-118`).
- **Theme (30 W4)**: stop the 2 s poll — `theme.name` is the one trigger; prove its watch survives a second `omarchy-theme-set` first, else watch the directory. Read the three absent files quietly. `tst_ThemeReload.qml`.
- Portal "Could not register app ID": nothing in the repo registers one — it comes from Qt or quickshell. Find out whether it can be silenced; if not, record it as known and remove it from plan 29's Verification.
- `Daemon.qml:60`: guard the dispatch even though the cause is fixed.
- **Video player** (½ day): reproduce the no-sound report with the default sink on the speakers. Then: a mute toggle, a scrub bar, and position/duration. Local files only stays, and the still says so for a remote one.
- The nvim bridge's socket is created without a 0600 mode and is not removed when the session closes (`openin.rs:133`, `256-266`): anyone on the machine who can reach it can drive the editor. Mode it, remove it.
- `jobs::wait` on a `Condvar` instead of a 5 ms poll (30 W5).

**Views**
- Gallery (D6): `Del` advances to the next image rather than the first; "7 of 31 images"; type-ahead off; `←` on the first image stays put.
- **Gallery in a folder with no pictures** (½ day). Today the stage falls back to a 96 px stroke glyph (`UI.Icon`, `GalleryPane.qml:254-268`) — not the artwork Icon view draws — and `selectFirst()` and the arrow keys look only for `image` / `video` rows (`:45`, `:84-93`), so in a folder of documents nothing is selected and nothing can be stepped through. Instead:
  - The stage draws the selected entry with **`UI.KindIcon`, the same folder and file artwork `IconPane.qml:114` uses, scaled to the stage** — about 40 % of the stage's shorter side, clamped to 128–320 px — with the name under it. A thumbnail, where the row has one (a PDF, a remote picture), still wins over the icon. This replaces the stroke glyph for a non-picture entry in a mixed folder too, so the stage has one look.
  - With no picture in the folder, `selectFirst()` takes the first entry, the arrows step through **every** entry, and the filmstrip shows them all with their `KindIcon`s. `Enter` / `→` on a folder goes into it and stays in Gallery; on a file it opens it, as now. In a folder that has pictures the arrows keep stepping pictures only, as now — the fallback is for a folder with none, not a change to what Gallery is for.
  - "No pictures here" is kept for one case only: a folder with nothing in it at all (and then it says "Empty folder", like the other views). The status count reads "N items" when there are no pictures and "7 of 31 images" when there are.
  - `tst_GalleryPane`: a folder of two folders and three documents selects the first entry, shows a `KindIcon` of the scaled size on the stage, steps through all five, and enters a folder on `Enter`; a mixed folder still steps only its pictures; an empty folder says so.
- Columns: a drag carries the selection, not just the row under the pointer (`ColumnsPane.qml:329`).
- Project mode (D2, 1 day, all **probable**): the default editor preset is `accepts="file"` so `e` on a folder launches nothing, and the error is swallowed; editor and agent share the class `kiki-tool`, so Arrange cannot tell them apart; nothing takes key focus in the mode, so `Ctrl+Shift+P` may not leave it; the agent respawns on re-entry. Fix, then one hand pass.
- `views.toml`: drop `view = "mirror"` from entries on load, keeping sort and hidden (plan 29 J's last unbuilt item).
- Emit `FavoritesChanged` and `Gone` (D10); `Hello` checks the version (D11).

### Phase 3 — jobs that can be watched, and the orb (4½–5 days) — **in progress**

**Done 2026-09-19: the daemon's half — what a job says about itself.** Suites after it: clippy clean; `cargo test` 158; QML 340; e2e 237 passed, 1 skipped. Still to do: the job log, then the orb and the popup.

- **`Job::json` carries the table below**, as planned, with these differences. `phase` and `cancelling` are *derived*, not stored: a running job with no totals and no bytes yet is `preparing`; a job whose cancel flag is up and which has not ended is `cancelling` — and `cancel()` now broadcasts, so the view hears it at once rather than when the worker looks up. `set_totals` broadcasts. `isDir` for something on a server is not knowable when the job is submitted (a URI keeps no trailing slash); the transfer says once it has listed it. `current` and `rate` are only reported while the job runs.
- **A folder copy counts files.** It counted one per top-level item against a total of *files*, so a folder copy ended at "1 of 458". `ops::Progress` gained an optional per-file callback (`FileEvent::Start` / `Done`); the local copy counts files as they finish and names the one in hand. A job that ends `done` ends at its totals (a rename is one step however many files it moves).
- **Mirror runs count as they go** — every change used to be reported as "nothing done", so a run sat at 0 of 900 until the end — and name the item most recently begun. With several workers the file's own bar is left out (`size` 0): the bytes moving belong to all of them. The counts go on the job as `result`.
- **`chmod` and Empty Trash** report item by item instead of one jump at the end.
- **A transfer's error names the file** (`site/img/c.bin: Denied`, not `Denied`). A download that stops part-way can be undone (`undo_so_far`), and says where to `Reveal`.
- **`copy_file_range` in 64 MiB steps**, not 1 GiB: that step is how often a cancel is looked for and progress reported (this was phase 4's).
- **`ClearJobs`, `DismissJob { job }`** and the event **`JobsCleared { jobs }`**; live jobs are never forgotten. `Jobs` returns every live job plus the fifty most recent finished ones — "the last fifty of everything" let a long transfer fall off the list while it ran.
- **Hidden**: `_silent` ops, `movePairs`, `rmdirIfEmpty`, `chmodList`, `mirrorScan`.
- **Tests**: `jobs::tests` (three new); and in `remote_transfers`, against real SFTP and FTPS: an upload says it is an upload, of which file, which file is in hand and its size; it is `preparing` and then `running`; a rate appears; `cancelling` is said while it is still `running`.
- Also, from a loose end after phase 2: the shell greeted the daemon twice at start — the first time before its socket object existed, which logged "Hello failed" on every launch. One deferred greeting now.

**Done 2026-09-20: the job log.** Suites after it: clippy clean; `cargo test` 162; QML 340; e2e 253 passed, 1 skipped. Still to do in this phase: the orb, the popup, and the log window.

- **The first thing done was to listen and read**, as promised — a small upload to each real server with every library at `Debug` — and the two could not have been more different. **suppaftp's `Debug` is a story** ("Put file …", "PASV command", "Renaming … to …"), a few lines a file. **russh's is the wire** ("> msg type 94, len 128", "packet type 101"): 380 lines for six small files, a line or three per 32 KiB of a large one — it would flood the ring and cost a big transfer CPU. rustls writes six per FTP data connection. So the level is **per library**: `russh`, `russh_sftp`, `rustls` (and `tokio`, `mio`) are heard from `Info` up, where their warnings and errors are; everything else at `Debug` while a job's session is open, `Info` otherwise; never `Trace`.
- **Every plugin tells the same story, from the SDK**: `connect` and `disconnect` (with the role), `write path (n bytes)`, `read`, `mkdir`, `delete`, `rename a -> b`, and any refusal as a warning with its code. That is what makes an SFTP log readable now that russh's chatter is out — 26 lines for the same six files, where there had been 380 — and gio gets it for nothing. Listing and stat are left out: a browser does thousands.
- **Whose line**: the SDK stamps each with the role of the session its thread is serving; a job's sessions are its own (`job-<id>`, phase 2), so that is whose it is. A line from a library's background thread carries none and goes to every job with a session open on that plugin just then (`locations::jobs_using`). The browser's lines are nobody's job and live only in the plugin's own log — **`LocationLog { location }`**, for a location that will not connect.
- **Secrets are taken out in the plugin** (`sdk::liblog::redact`): after the FTP `PASS` command, and after `password` / `passphrase` / `authorization` / `secret` / `token` *when a `:` or `=` follows*. A sentence that merely mentions one — "password authentication failed for gideon", the line somebody needs — is left alone; the first version ate it.
- **Forwarding is asked for, not assumed** (`KIKI_PLUGIN_LOG`, set by the daemon when it spawns a plugin): `Log` events arrive between other frames, a binary stream's included, and the FTPS plugin's own test — a strict reader — found one in the middle of a download. A host that did not ask gets none.
- **kiki's own lines** open and close the log: what was asked and from where to where; each file as it is begun, with its size; cancel requested; how it ended, with counts; a failure with the file's name.
- **Kept**: a ring per job and per plugin — 2,000 lines or 256 KB, saying how many earlier lines it let go — read with **`JobLog { job, from }`** → `{ lines: [{ t, level, source, text }], next, dropped }`, asked again from `next`. Gone with the job (`DismissJob`, `ClearJobs`, the trim). A **failed** job's log is appended to `~/.local/state/kiki/failed-jobs.log`, begun again when it would pass 1 MB.
- **Tests**: `joblog::tests` (routing by role, the ring, the failure file); `liblog::tests` (redaction, both ways); and on real SFTP and FTPS — the log opens with what was asked and closes with how it went, names each file, holds the part file being written and renamed into place, on that job's own session, is under 150 lines, has no password in it or in the location's log, reads on without repeating, and goes when the job is dismissed.
- **Differs from the plan above**: no `req` id on a line (the role does the job); `Debug` is not blanket; the mock-server tests do not assert on library lines (they do not ask for them) — the real-server flow does.



Plan 29 I placed the orb; decision 3 wants the spec's anatomy. The audit's finding is that **the daemon does not carry the data**: a job is `id, op, state, done, total, bytes, bytesTotal, title, error, undoable`, with a prose title and nothing about what is moving right now. So the daemon goes first.

**Daemon (1½ days)** — `Job::json` gains:

| Field | For |
|---|---|
| `name`, `count`, `isDir` | the entry's icon and header (the file or folder name, not "Copy 3 items to dst") |
| `src`, `dest`, `direction: "download" \| "upload" \| "local"` | "Downloaded 458 items" / "Uploaded 12 items" |
| `phase: "preparing" \| "running"` | the indeterminate bar. `transfer.rs` walks the whole tree before `set_totals`, so a remote job sits at 0/0; **`set_totals` must broadcast** (today totals arrive only with the next progress event) |
| `current: { name, bytes, size }` | the per-file detail row — set around `copy_file` in `transfer.rs` and `ops::copy_tree` in `jobs.rs`. With cumulative and per-file counters both sent, the spec's "detect the counter reset" trick is unnecessary |
| `rate` | bytes/s over a smoothed window, from the daemon, so every window shows the same number |
| `cancelling: true` | broadcast from `cancel()`; the header flips at once instead of when the worker notices |
| `result: { copies, deletes, skipped }` | the mirror's completion line, today only a toast string |
| `revealUri` | Reveal: the first local thing the job created |
| `hidden` | internal kinds (`movePairs`, `rmdirIfEmpty`, `chmodList`, `restore`, `mirrorScan`, silent undo deletes) stay out of the popup |
| the failing path in `error` | `VfsError::message()` gives a bare "Denied" for non-Io errors; plan 29 H needs the file named |

Also: mirror item counts move as actions finish (today `done` stays 0 until the end); `chmod` and `emptyTrash` report as they go rather than once afterwards; **`ClearJobs`** and **`DismissJob { job }`** — daemon-side, because `Jobs` is fetched again on reconnect and would bring dismissed entries back; `Jobs` never drops a live job out of its 50-entry reply. Rust tests for each.

**The job log (1–1½ days; D17)** — what the Log button opens.

- **SDK** (`kiki-plugin-sdk`, behind a `log` feature; one new dependency, the `log` crate, which is in `Cargo.lock` already through russh and suppaftp): a `log::Log` implementation installed by the SDK's `main` wrapper, so every storage plugin gets it without asking. Each record becomes a notification on the plugin's pipe — `Log { level, target, message, req }` — where `req` is the id of the request being served, held in a thread-local (and a tokio task-local for the SFTP plugin) set around each request handler. The gio plugin routes `glib`'s writer into the same sink.
- **Attribution**: the daemon knows which job issued `req`, and phase 2's job-role sessions mean a job's session serves that job and nothing else, so a line with no `req` — russh's background tasks, keepalives, a rekey — is attributed by *session*. Lines from a `browse` session belong to no job and are kept in a small per-location ring instead (the same window shows it from the sidebar's location menu: "Connection Log…"; that is the log one wants when a location will not connect).
- **kiki's own lines** go in the same log, so it reads as a story and not as a library dump: job started with its source and destination, each file begun and finished with size and time, each retry, skip and collision answer, the verify-before-delete of a move, cancel requested, the final outcome and error. Local jobs get these lines too — a local copy has a log, a short one.
- **Level**: `Debug` while a job session is open, `Info` otherwise; never `Trace` — russh's trace level is packet dumps, enough to slow a transfer and drown the rest. The logger's `enabled()` answers from an atomic, so a dropped record costs no formatting.
- **Secrets**: redact before the line leaves the plugin, not in the viewer — `PASS …` on the FTP control channel, anything following `password`, `passphrase` or `Authorization:`; a unit test feeds the redactor each. A log is something people paste into bug reports.
- **Storage**: a ring per job in the daemon — the last 2,000 lines or 256 KB, whichever comes first, with "… N earlier lines dropped" at the top when it wraps — kept as long as the job is kept and dropped with it (`DismissJob`, `ClearJobs`, the 100-finished trim). Not written to disk, except that a **failed** job's log is appended to `~/.local/state/kiki/failed-jobs.log` (capped at 1 MB, oldest out), so a failure from last night can still be read this morning.
- **Protocol**: `JobLog { job, from } → { lines: [{ t, level, source, text }], next, dropped }`, and `LocationLog { location, from }` of the same shape; while a log window is open it asks again from `next` on each `JobEvent`, so there is no second subscription to keep alive.
- **Log window** (`JobLogWindow.qml`): mono, timestamps relative to the job's start, level coloured (warnings yellow, errors in danger), library lines dimmer than kiki's own, follows the tail until scrolled up, a filter box, **Copy all** and **Save…**. Opened from the entry's **Log** button, and for a mirror from the Done screen beside "Save report…".
- **Tests**: the SDK logger against the stub plugin — a record inside a request carries its `req`, one outside carries none, `Trace` is never emitted, the redactor's cases; daemon — lines reach the right job with two jobs running on two sessions, the ring wraps at its cap and says so, a dismissed job's log is gone, a failed job's log is on disk; `mock_sftp.rs` and `mock_ftps.rs` each assert that a real transfer produces library lines *and* that the password appears nowhere in them; `tst_JobLogWindow.qml` — tail-following stops on scroll-up, filter, copy.

**Shell (2 days)** — `ActivityOrb.qml` and a rewritten `ActivityPopover.qml`:

- The orb exactly as plan 29 I: bottom right, in the window rather than the key-hint row; green and pulsing (`SequentialAnimation` on `opacity`, ~45 %–100 %, ~2.5 s) while anything runs; red with a white `!` when anything failed, outranking running, until the popup has been opened; dim and still otherwise, always present; tooltip in words. The 200 px click area goes; "N running ·" stays as text beside it.
- The popup anchored above the orb with its corner pointing at it; 400 px wide, height to fit, scrolling; header "Activity" with **Clear** (finished entries only); "No activity" when empty; closes on `Esc` and an outside click; a click on the orb within 250 ms of that close counts as the close.
- Entries from the spec: icon, bold header, 5 px bar (indeterminate while preparing or cancelling), dim status line, a disclosure that opens the current file's own bar and "12 KB of 254 KB (45 KB/sec)" once bytes move; when finished, a wrapping completion line. Entries keep their place — never re-sorted on finishing. Delete and chmod entries remove themselves on success. Buttons: **Log** on every entry, running or finished (opens the job log, above); **Cancel** while unfinished; **Reveal** for a finished transfer with a `revealUri` (selects it in the focused pane — kiki is the file manager); **Dismiss** otherwise. A failed entry's error line ends in "— see Log".
- Status lines by case — preparing; single file (bytes and rate); many files ("12 of 458 transferred — 2% complete"); mirror ("120 of 900 items · 40.2 MB of 1.10 GB · 2.1 MB/sec" with the current path and action); item-count jobs ("Processing 12 of 458 items"); cancelling. Sizes through `Format.bytes`, brought to the spec's rule (bytes under 1 KB, KB whole, MB one decimal, GB two). Error text with nested type prefixes stripped, "Failed" when empty — one pure function in `Format`.
- No poll timer: events arrive every 100 ms. Rebuild only while visible; hidden, update the model and the orb.
- Not applicable, and the new plan says so: the spec's child-job rule — a mirror is one job and per-action failures are "skipped".

**Tests**: `tst_ActivityOrb.qml` as plan 29 I lists it; `tst_ActivityEntry.qml` for every status and completion format, the disclosure, Reveal versus Dismiss, Clear leaving running entries, self-removing delete/chmod, hidden kinds absent, order stable; `tst_Format` for sizes and error stripping.

**Docs**: `32-activity.md` from plan 29 I plus the above; delete `activity-view-spec.md`; plan 04's Activity paragraph points at it.

### Phase 4 — large transfers (3½–4 days)

Plan 29 H, less what exists. `remote_transfers.py` has the servers and the nine pairs; it has six small files and asserts none of H's list.

- **Move semantics first** (½ day; `transfer.rs:188-190`): a remote move deletes the original after the copy with **no verify step** — verify size (and mtime within the plugin's tolerance) before the delete. A delete half that fails reports "copied; the original could not be removed", never a failure that implies nothing arrived. **Decide stop-or-carry-on** when one file fails mid-copy (today: stop at the first error): carry on, fail the job at the end naming each file, as mirror does.
- Lift `Servers` (the user-mode `sshd` and FTPS fixtures, `add_location` with trust) out of `remote_transfers.py` into the harness.
- `kikid bench gen transfer` (½ day): ~50,000 small files in ~2,000 directories eight deep, two or three files of 1–4 GB, empty directories, a symlink, a newline and a non-UTF-8 name, a read-only file, a zero-byte file, mtimes across years. Today every profile's files are 1–6 bytes.
- `tests/e2e/transfer_common.py`: tree hasher (full for large, sampled for small); a `JobEvents` recorder asserting bytes monotonic, totals never revised down, an end at 100 %; cancel mid-run with no partial file under its final name; failure injection (the read-only file; `kill` the `sshd`); daemon peak RSS from `/proc`; throughput written in the bench JSON shape beside plan 26's baselines.
- `transfer_local` (same filesystem and across, reusing `cross_fs.py`), `transfer_download`, `transfer_upload` — out of the default run, by name, like `gallery_perf`. Each also asserts, with the job running, that another folder's listing answers inside its budget, and that the orb is green, goes red on the read-only file until the popup is opened, and that the popup's totals are the daemon's.
- **Expect these to break**: local copy uses `copy_file_range` in chunks of up to 1 GiB (`ops.rs:44`), so cancel and progress on a 4 GB file are coarse — cap the chunk (64 MB); remote `copy_file`'s partial-file cleanup on cancel has not been checked; a cancelled or failed big copy journals nothing, so its partial result cannot be undone — journal what was created. Half to one day is held for this.
- Extend `remote_transfers.py` (½ day): remote `delete`, `rename`, `mkdir`; collisions (replace, keep both); cancel; mtimes compared.

### Phase 5 — drag and drop (2½ days)

**What is there**: all four views take drops; List, Icon and Columns are sources; `Pane.dropAction` decides — `Ctrl` copy, `Shift` move, move within a scheme and host, copy across — and `tst_DropAction` covers the table for nine pairs of ends; a drop is one job through `Ops.transferTo`, and works for remote ends.

**To build**
- Gallery as a drag source. (Columns carrying the selection is in phase 2.)
- `Ctrl+Shift` link — local → local only, not offered otherwise. `Alt` and a right-button drag: drop, then a menu at the pointer — Copy here / Move here / Link here / Cancel. (Check that the compositor delivers a right-button drag through `Drag.Automatic` at all.)
- **Shown while dragging**: a one-line hint by the pointer ("Copy to homelab:/srv/site") and the cursor badge, changing the moment a modifier goes down or the target changes; modifiers read continuously and at drop, never captured at the start. A refused target — read-only location, a folder into itself — shows the "no" cursor *during* the drag.
- The drop moves focus to the pane that received it; the press that starts a drag neither focuses the other pane nor disturbs the selection being dragged.
- Trash: remote files dropped on it **ask, in danger colours** — a remote has no trash. Today the sidebar submits `trash` for any URL.
- Favorites stays "drop adds a favourite" (`Sidebar.qml:65-75`); plan 29 K's "favourites are drop targets for files" is wrong about the code, and is amended.
- `Esc` cancels and changes nothing.
- **Not in 0.1.0**: spring-loading — no hover-to-open, no `springDelayMs`, no `tst_SpringLoad`.

**Tests**: `tst_DragDrop` / `tst_DropAction` grow link, ask, refused targets, remote-to-trash, modifiers at drop time, focus to the receiver. **IPC `shell drop <uris-json> <dest> <modifiers>`** calling `Pane.dropInto` with a drop-shaped object — the pattern `sideBySide drag` already uses — and **`tests/e2e/flows/drag_between_panes.py`** on it: every local / SFTP / FTPS pair and each modifier, asserted on disk, under `cage`. What stays manual is the physical press–move–release, and it goes on the phase 10 checklist.

**Sway spike** (½ day, time-boxed): `run_in_sway()` in `run.sh` (`output HEADLESS-1`, `exec` the script, `swaymsg exit`), to find out whether sway's headless backend delivers virtual-pointer events to Quickshell. If yes, `pointer_ops` (14 click checks) stops skipping and `sway` joins CI. If no, the skip stands and `11-testing.md` records why. Either way the answer is written into plan 11.

### Phase 6 — Side by Side and mirror, driven (3½ days)

- **By hand first** (½ day + fixes), plan 29 A's three lists — plans 07, 08 and 24 as amended by 29 J — on two local folders, then a real remote. Cancel a mirror mid-run and confirm the browser still lists (phase 2's job sessions are what make that true).
- **`MirrorWorkspace` has no IPC hooks at all** (only `mirror open|close` and `mirrorScreen()`). Add: preflight, set an option, run, answer the blast-radius prompt, read counts and the Done summary. Then `mirror_local.py` (1 day): scan → plan → run → done, the destination tree and the report, second run empty, deletes-on, each guard refusing.
- `mirror_sftp.py` (½ day) on the phase 4 fixture: idempotence with spread source mtimes; cancel mid-run, then list. FTPS mirroring: one run in the same flow where pyftpdlib is present.
- Finish what plan 29 A called `split`, inside `side_by_side.py` (½ day): cross-pane copy and its undo, cross-pane move and its undo, selections back after collapse and expand, a location opened side by side, the last location on `Ctrl+4`, a remote URI from the breadcrumb pairing with its local folder.
- Small tests (½ day): `pick_detector` per plugin and direction; a plugin killed mid-`Read` is respawned and the job says so; plugin-supplied `Meta.hidden`; the view-by-contents default (D18).

### Phase 7 — test debt that guards shipped behaviour (3½ days)

Chosen because each covers something a user relies on and nothing tests. What plan 11 promised beyond this is D20.

- **Watch** (½ day): touch, create, delete → the row changes inside the budget; two clients both told; eviction at 64 watches keeps `Meta`.
- **Undo round trips** (½–1 day): rename, mkdir, chmod, same-filesystem move, restore, compress, extract; undo after a daemon restart (the journal persists; nothing proves it).
- **JSON** (½ day): a fuzz or property target for `kiki-json`; a round trip of each protocol message type.
- **Git badges** in QML and one e2e check: each state's letter and colour; the folder's aggregate; a commit from outside clears them.
- **`tst_SettingsWindow`** (½ day): default view lands in `settings.toml`; an invalid value is refused inline; Reset all; the AI provider.
- **Share** (½ day): a contract test that runs the three share binaries; `share.rs` against a stub.
- **Open in**: one QML test for the menu and `Alt+Enter`; absent binaries hidden.
- **Small**: resolver (`~`, an unknown scheme as a typed error); local `mkdir` / `rename` / `delete` / `set_mtime`; the 1,000-folder view-memory cap actually exceeded; heat-map compaction at 50,000 lines and the `atime` sort; octal and symbolic permissions against `stat -c`; redo (`Ctrl+Shift+Z`) in a flow; paste, new folder and drop disabled in the trash view.
- **Cheat sheet**: `ShortcutsOverlay` generated from `Keymap.qml`, and `config.rs`'s third copy of the key table removed — or, at the least, the three made to agree (mirror view → Side by Side, a `Ctrl+5` gallery row, Backspace, copy path).

### Phase 8 — real servers, real devices (1–2 days, mostly waiting on hardware)

One session each, against plan 29 B's list — list, copy out, copy in, rename, delete, mkdir, mtime kept, mirror second run empty, wrong password, disconnect while open:

- **SMB share** and **WebDAV server** through gio. The plugin has never met a server and has no tests; a day is held for what breaks.
- **SFTP sign-in** (plan 29 L) against a real server: keys, password, keyboard-interactive, the trust step, Add versus Add and Connect.
- **LocalSend to a phone.** If it works it comes on by default (drop `off_by_default`); if not it ships off, as now.
- Mail through Thunderbird's composer; Tailscale to a second node.

### Phase 9 — the documents agree with the build (1 day)

- A **Status** line under every plan's title (table below).
- `CORE.md`: both open questions are answered — the PKGBUILD depends on `gnome-keyring` and `libsecret`; kiki's chooser sits *beside* GTK's, per user, with gtk as the fallback — so record them under Decisions and empty the section. Replace the stale plan-17 decision with decisions 1–6, and add: **kiki has no built-in code viewer and will not grow one** — a decision about what kiki is, recorded under Decisions rather than "Out of scope for 0.1.0", so nobody reads it as "later". Row 13 of the build order becomes "Editor bridge", and `CORE.md:80`'s mention of the viewer goes. "Out of scope for 0.1.0" gains: MTP, AFC and PTP; the Jarvis panel; Messages and AirDrop; spring-loaded folders; LocalSend receiving; Swap, "last mirrored" and the mirror options; the divider nudge; the pane-header branch chip; `ssh-agent` and `~/.ssh/config`; the `smb://` handler; AFP; and whichever of D1–D23 stand. Fix the architecture box (kinds), the `vsftpd` prerequisite, and rows 17, 18 (garbled), 19, 24, 29.
- `README.md`: Status rewritten from the suites' real numbers; Jarvis out of line 5; the kinds line; an install section, the keymap, and the location and mirror workflows (plan 10's "Docs").
- `API-DAEMON.md`: add `AiOpen`, `OpenTerminal`, `SetLocationImage`, `JobEvents`, `ClearJobs`, `DismissJob`, `JobLog`, `LocationLog` and the new job fields; remove `AiQuery`, `AiCancel`, `IndexProgress`, `SharePluginsChanged`, the `Read`/`Write` streaming text; `compress.format` gains `tar.bz2`. `API-PLUGIN.md`: the `Log` notification and what a plugin must never put in it; `Field.page`, `kind: "location"`, the share `Describe` fields (`requires`, `defaultEnabled`, `unavailable`), `smb`/`dav` as shipped, and the truth about `LOCATION_KINDS` — a dropped-in location plugin is ignored by a release build.
- Plan amendments: 01 (paths, the `Backend` trait as it is, Prefetch struck); 02 (Side by Side, IPC target `shell`, the share and AI rows); 03, 04 (activity → plan 32); 05; 06 (kinds, agent auth and `splice` removed); 07, 23, 24 (the rename; 24's reversed Verification line; D18, D19 and plan 23's five other doc/code disagreements); 08 (entry point, Edit rules…); 10 (dependency lists); 12 (D5); 13 (retitled "Editor bridge": the viewer, the grammars, `OpenText`/`TextFind` and the Code tab's Verification bullets are deleted, with one line saying why; what is left is the nvim session, `F4`, and its gaps — `follow` stored and never read, no fallback to the `system` entry when `nvim` is absent, the socket not removed on close nor made 0600 — of which the socket is fixed in phase 2 and the rest are post-0.1.0); 03 and 23 lose their Code-tab lines, and 03 its "editable name in the header" (D7); 14, 16, 20 (what is post-0.1.0); 15 (toast); 18 (UI section); 21 (gallery, off while side by side); 22 (the `t^1.5` formula, the Accessed field); 27; 29 K (Favorites, undo, spring-loading) and 29 H (what exists).
- Plan 19 becomes a stub: *not in 0.1.0 — replaced by "Open AI here…" (plan 29 L2)*.

### Phase 10 — package, install, check, tag (1 day)

- `cd packaging && makepkg -f` on a clean checkout; install the package; everything below is run against the **installed** kiki.
- Launch checks from phase 2: a second `kiki` raises the first window; `xdg-open ~/` opens kiki and a file keeps its own handler; nothing was set for root.
- Plan 29 D's checklist, plus: a physical drag between the panes, local → remote, of a few thousand files with the orb up, cancelled halfway, both ends checked; drag out to a terminal and in from a browser; `Alt` and right-button drags; every theme eyeballed once (it stands in for D20's visual layer); `omarchy-theme-set` twice with kiki open; project mode once (D2).
- A session's shell log free of plan 29 F's warnings (or the portal one recorded as not ours).
- Release notes: what ships; what is hand-verified only (gio; FTPS mirroring if pyftpdlib was absent; the physical drag); what is not in 0.1.0. Tag.

**After the tag, 0.1.1**: 30 W6–W8 on a quiet tree, one commit each, before/after screenshots identical.

## Size

| Phase | Days |
|---|---|
| 0 Clean tree | ½ |
| 1 Build matches decisions | 1½ |
| 2 Defects | 5 |
| 3 Jobs, the job log and the orb | 4½–5 |
| 4 Large transfers | 3½–4 |
| 5 Drag and drop | 2½ |
| 6 Side by Side and mirror driven | 3½ |
| 7 Test debt | 3½ |
| 8 Real servers and devices | 1–2 |
| 9 Documents | 1 |
| 10 Package and tag | 1 |
| **Total** | **28–30** |

If that is too long, the cut line, in the order it costs least: phase 7 down to watch, undo and git badges (−2); `transfer_download` and `transfer_upload` folded into one remote flow (−½); D8 to a doc amendment (−½); the sway spike dropped and the skip accepted (−½). Phases 1–3, the move-semantics fix in 4, and phases 5 and 10 are not cuttable: they are the decisions taken, or defects that lose data.

## Status lines (written into each plan in phase 9; "unverified" must be gone, or explained, by the tag)

| Plan | Status at the tag |
|---|---|
| 01 Daemon and listing | built and tested; perf budgets measured in `bench`, not asserted; Prefetch struck |
| 02 Shell and views | built and tested; launch-to-paint timing unmeasured |
| 03 Inspector | built and tested; Created field not in 0.1.0; the name is read-only by decision (rename lives in the views) |
| 04 Operations and undo | built and tested; parallel small-file copy not in 0.1.0; activity → plan 32 |
| 05 Archives | built and tested |
| 06 Remote locations | built and tested against real `sshd` and FTPS; agent auth not in 0.1.0 |
| 07 Split mode | built and tested as Side by Side (plan 29 J) |
| 08 Mirror | built and tested, local and SFTP; Edit rules… not in 0.1.0 |
| 09 Omarchy integration | built; portal and FileManager1 hand-verified on the installed package; D-Bus activation not in 0.1.0 |
| 10 Polish and packaging | built; installed from the package once |
| 11 Testing | daemon, QML and e2e layers built; visual, layout and shell-performance layers not in 0.1.0; pointer e2e per phase 5's answer |
| 12 Search | built and tested as trimmed |
| 13 Editor bridge | no built-in viewer, by decision; nvim bridge built, hand-verified |
| 14 Open in | core built and tested; Settings page, watcher, `tab`, `follow` not in 0.1.0 |
| 15 Git status | built and tested |
| 16 Project mode | built, hand-verified; the rest of the plan not in 0.1.0 |
| 17 Devices | not in 0.1.0 (plugins stay in the tree; detection off) |
| 18 Share | built and tested; each way sent once for real |
| 19 AI query | not in 0.1.0 |
| 20 Settings | 8 pages built and tested; Locations, Open in, Plugins not in 0.1.0 |
| 21 View memory | built and tested |
| 22 Access heat map | built and tested; Accessed field not in 0.1.0 |
| 23 UI refinement | built and tested |
| 24 Mirror view | built as Side by Side; mirror bar, Swap, "last mirrored" dropped |
| 25 SMB | built, hand-verified against one SMB and one WebDAV server; no automated test |
| 26 Benchmarks | daemon half built and in CI, with a transfer profile; shell half not in 0.1.0 |
| 27 Gallery | built and tested; zoom, video poster, remote large preview not in 0.1.0 |
| 28 UI test coverage | built; drags by IPC hook and by hand |
| 29, 30 | closed by this plan; 30 W6–W8 in 0.1.1 |
| 32 Activity | built and tested |

## Verification

- `make test` is green, offscreen, with clippy at zero warnings inside it; the e2e default run includes `side_by_side`, `mirror_local`, `remote_transfers`, `drag_between_panes` and, where `sshd` exists, `mirror_sftp`; `transfer_local`, `transfer_download` and `transfer_upload` pass by name on the large fixture with throughput and peak RSS recorded beside plan 26's baselines.
- `plugin::LOCATION_KINDS` is `ftps`, `sftp`, `smb`, `dav`; each has been opened against a real server once; no Devices section shows; every kind not shipped is under "Out of scope" in `CORE.md`.
- No `kiki-plugin-highlight`, `OpenText` or `TextFind` anywhere in the tree or the package, and no plan promises a code viewer.
- No `AiPanel`, `AiQuery` or "Jarvis panel" anywhere in the tree; "Open AI here…" opens a terminal with the chosen tool.
- The orb shows idle, running and failed correctly through a transfer that is cancelled and one that hits a read-only file; the popup's entry shows the current file, its bytes and a rate, ends on the right completion line, and Reveal selects what arrived. Its Log shows kiki's own lines and the SSH or FTP library's beneath them, for that job and no other, with no password anywhere in it; a job that failed overnight can still be read from `failed-jobs.log`.
- Dragging between the panes: the default and each modifier match plan 29 K's tables for every local/remote pair, shown while dragging; the receiver takes the focus; remote files on the trash ask first. Nothing springs open.
- A remote move verifies before it deletes; a cancelled upload leaves the browser listing; undoing an extraction into an existing folder removes only what it added.
- `git commit` in a terminal clears the badges and updates the chip without a re-list; the column view shows the same marks as the list.
- Installed from the package: a second launch raises the first window; root's MIME defaults are untouched; plan 29 D is ticked line by line.
- Every plan in `docs/0.1.0/` carries a Status line matching the table above, `activity-view-spec.md` is gone, and `README.md`, `CORE.md` and both API documents describe the build that was tagged.
