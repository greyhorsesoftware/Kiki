# 30 — Code health: what the 2026-09-19 scan left open

**Status:** W1, W2 and W3 built and tested 2026-09-19; W4–W8 not started. Source: `29-release-readiness.md` section M. Two findings were fixed the day of the scan (the column view's leaked listings, and `otherPane` shadowing its own function) and are not repeated here.

Nothing here changes what the user sees, except W1 (a LocalSend send to an impostor now fails) and W3 (git badges become right after `git init`). Every item has a test that fails before the change; "it looks tidier" is not a reason to ship any of it.

## Order and size

| # | Item | Why this place | Size |
|---|---|---|---|
| W1 | LocalSend checks who it is sending to — **done** | Security; files leave the machine | ½ day |
| W2 | Mirror plans are dropped — **done** | The largest leak: a plan lists every file on both sides | 2 h |
| W3 | Git caches are bounded and invalidated — **done** | A leak and a wrong answer | 3 h |
| W4 | Theme stops polling | Every window, every 2 s, for ever | 2 h |
| W5 | Small carelessness | Cheap, rides along | 1 h |
| W6 | `Label.qml`, `Rule.qml` | 220 repetitions; mechanical but wide, so last and alone | ½ day |
| W7 | Shell's view components, long lines | Touches `Shell.qml`, which everything else touches too | 3 h |
| W8 | Rust repeats | Small | 1 h |

W1–W3 go into 0.1.0. W4–W5 should. W6–W8 are for a quiet tree: they conflict with anything else in flight, so do them right after a commit and commit them on their own, one per item.

---

## W1. LocalSend checks who it is sending to

**Now:** `plugins/kiki-plugin-share-localsend/src/http.rs` installs `AcceptAll`, a verifier that approves every certificate. Discovery (`main.rs`, the multicast loop) reads each peer's announced `fingerprint` only to skip ourselves, then throws it away: a target's id is `proto://ip:port`. So a send goes to whoever answers on that address with any certificate at all.

**The protocol's rule** (LocalSend v2, HTTPS): a device's `fingerprint` is the SHA-256 of its certificate, in hex. That is the identity check; there is no CA.

**Change**
1. Keep the fingerprint with the target. Put it in the id — `https://192.168.1.20:53317#<hex>` — so it travels through the daemon and the shell untouched and comes back in `send`. `parse_target` splits it off; an id without one (typed by hand, or a plain-HTTP peer) has none.
2. Replace `AcceptAll` with `Pinned(Option<[u8; 32]>)`: `verify_server_cert` hashes the end-entity DER with `ring::digest::SHA256` (already a dependency) and compares in constant time; a mismatch is `rustls::Error::General("this is not the device that announced itself")`. Signature checks stay delegated to rustls as now.
3. `connect` takes the expected fingerprint. HTTPS with none → refuse: "this device did not say who it is". Plain `http://` peers keep working (the protocol allows it, and there is nothing to verify), but the target's `detail` says "unencrypted".
4. The refusal reaches the share sheet as the plugin's error text, as refusals already do.

**Tests** (`plugins/kiki-plugin-share-localsend/tests/`): an in-process rustls server with a generated certificate. Right fingerprint → the prepare-upload request arrives. Wrong fingerprint → the send fails and **the server saw no HTTP request at all** (the point: nothing is sent before the check). HTTPS with no fingerprint → refused. `parse_target` round-trips ids with and without `#`, IPv6 included.

**Also:** the rustls verifier boilerplate is copied in `kiki-plugin-share-mail/src/smtp.rs` (`Pinned`). After this both plugins hold the same "pin by SHA-256" verifier: move it to `kiki-plugin-sdk` behind a `tls` feature and use it from both (and see whether FTPS's `PinVerifier` can sit on it — it also falls back to web PKI, so perhaps not). This is W8's only non-trivial part; do it here while the code is open.

**Built (2026-09-19).** As above, with one change of mind: **an HTTPS address typed by hand is not refused.** It announced nothing, so there is nothing to hold it to, the user named the address themselves, and refusing would end sending across subnets where discovery does not reach. It is sent to as before — and can be pinned by writing `address#fingerprint`. A discovered HTTPS device always carries its fingerprint and is always held to it; a `#` followed by anything but 64 hex digits is an error rather than "no fingerprint", so a pinned send cannot quietly become an unpinned one. The comparison is a plain one (both sides are hashes of a public certificate). `tests/pinned_tls.rs` runs the plugin against an in-process TLS receiver: the announced device is sent to; an impostor is refused **with zero bytes of HTTP reaching it**; the hand-typed address works; the mangled fingerprint is `Invalid`. Moving the verifier into the SDK with mail's was **not** done — still W8.

## W2. Mirror plans are dropped

**Now:** `kikid/src/mirror/store.rs` — `store(job, spec, plan)` inserts into a global map; nothing removes. Readers: `jobs.rs:598` (a run started from a reviewed plan), `server.rs:334` (`MirrorReport`), `server.rs:625` and `:853` (the plan as a windowed listing).

**Change**
- `store::forget(job)`. Called when the client that opened the plan's listing closes it (`Server::close` already clears `self.plans` — find the job id there) **and** when that client disconnects (`Server::run`'s teardown, line ~91, walks `self.plans` the same way it walks listings).
- A run started from a plan holds its own `Arc<Stored>`, so forgetting mid-run is safe; `MirrorReport` after the listing is closed answers "no such plan", which the shell already handles.
- Belt and braces: keep at most 8 plans, dropping the oldest job id on insert. A client that crashes between scan and close must not pin a million-row plan for the life of the daemon.

**Tests** (`kikid/src/mirror/tests.rs`, `kikid/tests/`): scan → open → close → `stored(job)` is `None`. Scan → disconnect → `None`. Nine scans → the first is gone, the ninth is there. A run in flight survives `forget`.

**Built (2026-09-19)**, by counting rather than by forgetting on close — closing alone is wrong: a run is *queued*, and if the user leaves the workspace before it starts, the plan it was started from would be gone ("no such plan"). So `mirror/store.rs` keeps a plan while **either** a client is showing it (`view` / `unview`, counted; `Server::close`, a re-used lid, and the client's teardown all `unview`) **or** a run started from it is queued or running (`jobs::plan_wanted`); the run drops it when it ends, however it ends (`run_finished`, from a drop guard), unless it is still on screen — where "Save report…" needs it. Plus the cap: `KEEP = 8`, oldest idle first, never one on screen or being run. Four tests in `mirror/tests.rs`; the e2e mirror flows pass over it.

## W3. Git caches are bounded and invalidated

**Now** (`kikid/src/git.rs`)
- `status_cache`: `HashMap<PathBuf, Status>`, one per directory ever listed, each holding every changed path under it. Removed only by `invalidate(dir)` (a rescan of that directory, or the explicit request at `server.rs:322`).
- `repo_root`: `HashMap<PathBuf, Option<PathBuf>>`, one entry per directory ever visited, negatives included, never invalidated — so after `git init`, `git clone` into a visited folder, or deleting `.git`, the answer is wrong until the daemon restarts.

**Change**
- One small `Bounded<K, V>` in `git.rs`: a `HashMap` plus insertion order, capped (status: 64 directories; roots: 1024), oldest out. Not an LRU crate — twenty lines.
- When the listing cache evicts a directory (`listing/cache.rs::evict_if_needed`) it calls `git::invalidate(path)`: a status nobody is looking at is not worth keeping.
- `repo_root` entries carry an `Instant`; a negative answer is trusted for 5 s, a positive one for 60 s and only while `<root>/.git` still exists (one `stat`). `invalidate(dir)` drops the root entry for `dir` too, so a rescan after `git init` picks the repository up at once.

**Tests:** list 100 temp directories → `status_cache` holds ≤ 64. A directory listed before `git init` shows states after it without a restart. Remove `.git` → badges go. `bench.rs`'s git numbers do not move (the hot path is still one map lookup).

**Built (2026-09-19).** No `Bounded` type in the end: `Status` already carries `at`, so `status()` drops the oldest while over `STATUS_KEEP = 64`; roots are `(answer, Instant)`, believed 5 s when negative and 60 s when positive **and only while `<root>/.git` exists**, capped at `ROOTS_KEEP = 1024` (stale ones out, then all — an answer is a handful of stats to get back). `invalidate(dir)` drops the root too, and the listing cache calls it when it evicts a local directory. Tests in `git.rs`: late `git init` noticed; `.git` removed → not a repository at once; 72 directories → never more than 64 statuses, and a dropped one comes back when asked; roots capped. **Not measured:** `bench.rs`'s git numbers, and the VmRSS check under Verification.

## W4. Theme stops polling

**Now:** `qml/kiki/Theme.qml` — a 2 s repeating `Timer` reloads `omarchy`, `themeName`, `iconsFile` (and `legacy`) for ever, in every window, because a `FileView` watch on a file behind Omarchy's `current` symlink is on the old inode once the theme changes.

**Change:** `themeName` (`~/.local/state/omarchy/current/theme.name`) is already watched and changes with every theme switch. Make it the one trigger: on its `fileChanged`, reload it and the others, and re-arm — then delete the timer. First prove the premise on a real `omarchy-theme-set`: that `theme.name`'s watch fires on every switch, the second included (if the watch itself is on a replaced inode after the first switch, watch its directory instead, or re-create the `FileView` on each change). If no watch survives, keep a timer but make it cheap: `stat` one file's mtime every 2 s and reload only when it moved.

While there: the three reads of files that do not exist on current Omarchy (F's third bullet) — check existence once, or set `printErrors: false`, so a start is quiet.

**Tests:** `tst_ThemeIcons.qml` / a new `tst_ThemeReload.qml` with a temp state dir: rewrite the files, touch `theme.name`, the palette follows; twice. No `Timer` with `running: true` left in `Theme.qml` (or, in the fallback, none that reads a file's contents). By hand: `omarchy-theme-set` twice with kiki open.

## W5. Small carelessness

- `kikid/src/listing/mod.rs`: delete the empty `impl Inner {}`.
- `kikid/src/jobs.rs:676`: `wait` polls a job's state every 5 ms. Give `Job` a `Condvar` beside its status, notified wherever the state is set; `wait` blocks on it with the timeout. (`listing/mod.rs:147`'s 2 ms loop is tests and bench only — leave it.)
- The eight clippy warnings: `cargo clippy --fix`, then read the diff. Add `cargo clippy --all-targets -- -D warnings` to `make test` so they stay at zero.
- `make test-qml` runs on the developer's desktop, where the pointer makes it flaky: set `QT_QPA_PLATFORM=offscreen` in the Makefile target.

## W6. `Label.qml` and `Rule.qml`

**Now:** `font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize` with a colour is written out 201 times; a one-pixel `Rectangle` in `Kiki.Theme.line` 19 times in three orientations.

**Change**
- `qml/kiki/ui/Label.qml`: a `Text` with the mono family, the theme size and `fgDim`; `dim: false` for `fg`, `size` for the few that differ. `FieldLabel.qml` for the 11 px upper-case, letter-spaced one used over form fields.
- `qml/kiki/ui/Rule.qml`: `edge: "top" | "bottom" | "left" | "right"`, anchored and sized from that.
- Replace by script, file by file, **only where the line matches the pattern exactly**; anything with an extra property on the same line is left for a hand pass. Do not touch `Text` items whose font is bound to something else.

**Guard:** the e2e harness already screenshots; take the full set before and after and compare — the rule is *no pixel moves*. Run the QML suite after every file, not at the end. If a replacement needs thought, it is not part of this item.

## W7. Shell's view components and long lines

- `Shell.qml`'s `listView` / `iconView` / `galleryView` / `columnsView` repeat `onActivate` and `onContextMenu` bodies. Move the bodies to `win.activate(pane, i)` and `win.showContextMenu(pane, i, pos)`; the components keep one-line handlers. Gallery's different activate (open externally) stays a flag, not a copy.
- The sidebar's location menu (`onEditLocation`, one 500-character line) becomes `win.locationMenuItems(loc)`, next to `locationImageItems`.
- The 31 lines over 300 characters (`MirrorWorkspace.qml` 13, `SettingsWindow.qml` 10, `Shell.qml` 8): break each at its object boundaries. Formatting only — no behaviour moves in the same commit.
- The text-input styling shared by `Breadcrumb`, `FormField`, `SearchBox`, `ListRow`: a `LineInput.qml`. The dropdown chevron in three places: into `Icon` users via a `Chevron.qml`, or leave — three is not many.

## W8. Rust repeats

- `server.rs`: the `Count … done` event built identically at 202, 710, 859 → `fn count_done(&self, lid, n)`. The `uris` array parse on adjacent lines 282/287 → `fn uris(b: &Value) -> Vec<Uri>`.
- `locations.rs`: `location.get("config").cloned().unwrap_or(empty)` three times → `fn config_of(&Value) -> Value`.
- The TLS verifier: done in W1.
- **Not duplication:** the storage plugins repeating `scan` / `read` / `write` signatures. That is the SDK's trait.

## Verification

- `make test` green, with clippy in it at zero warnings and the QML suite offscreen.
- LocalSend: a send to a peer whose certificate does not match what it announced fails before one byte of HTTP is written.
- The daemon's memory after walking 5 000 directories inside repositories and scanning ten mirror plans returns to within a few MB of where it started once the windows are closed (`/proc/<pid>/status` VmRSS, recorded in `26-benchmarks.md`).
- `git init` in a folder kiki has already shown → badges without a restart.
- `strace -c -p <qs pid>` over ten idle seconds shows no periodic `openat` of theme files.
- W6/W7: before/after screenshots identical.
