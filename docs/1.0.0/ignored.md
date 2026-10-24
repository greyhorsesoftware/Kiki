# Ignored — what the fork changed and kiki will not

The companion to `01-from-the-fork.md`, which holds only work to do. This file holds the rest:
the parts of [ChrisTitusTech/Path](https://github.com/ChrisTitusTech/Path) that were read,
understood and declined, with the reason. Recorded 2026-10-24 so that nobody goes looking for
work that is already decided against, and — now that the work is built — so that a later
reading of these commits does not quietly undo a decision by following the fork's diff.

Two kinds. **Declined outright** is a whole commit with nothing in it for kiki. **Declined in
part** is a line inside a commit kiki *is* porting, where the fork's version would be a
regression here — the plan states the requirement affirmatively and does not mention the
alternative, so the reasoning lives here, with the plan item each one constrains.

---

## Declined outright

### `7fbb9bd` — fix(clippy): address Rust 1.99 compatibility

Adds `#[allow(deprecated)]` above four `AtomicUsize::fetch_update` calls and changes
`map_err(&cert_err)` to `map_err(cert_err)` twice. Kiki has none of it:

- The three `fetch_update` sites the fork patched are, in kiki, hand-written
  `compare_exchange_weak` loops called `one_off` — `crates/kiki-plugin-sdk/src/lib.rs:575`,
  `kikid/src/mirror/store.rs:15` and `plugins/kiki-plugin-sftp/tests/mock_sftp.rs:120` — each
  with the reason in a comment dated 2026-10-04: `fetch_update` is deprecated as of Rust 1.98,
  `try_update` (the name replacing it) is not in the release before, and kiki's builders are
  not all on the same toolchain, so the loop both names wrap is written out and compiles on
  either. The fork is patching code kiki deleted.
- `plugins/kiki-plugin-ftps/src/main.rs:313` and `:320` already pass `cert_err` by value.

Kiki's answer is the better one and should stay: `#[allow(deprecated)]` goes stale the day
`fetch_update` is *removed* rather than merely deprecated, and the explicit loop does not.

**Nothing to do, now or later.**

---

## Declined in part

### The local→local byte loop — `849202a` (constrains plan item 3)

On its way to staging through `tempfile`, the fork replaced the local→local mirror branch's
`crate::ops::copy_file` with a 512 KiB `read`/`write_all` loop:

```rust
while let Some(chunk) = read_chunk(&mut source)? { tmp.write_all(&chunk)?; … }
```

That discards `ops::copy_file` (`kikid/src/ops.rs:61-`) entirely: `copy_file_range`, which is
an in-kernel copy and a reflink on btrfs and XFS; the `fallocate` and
`posix_fadvise(SEQUENTIAL)` ahead of it; and the 64 MiB cancel-and-progress granularity, which
was itself a deliberate change from 1 GiB because four progress updates on a 4 GB file made a
cancel slow to notice. On this tree that is the measured fast path for a local mirror, so
copying the fork here would be shipping a known regression on purpose.

Kiki takes the confinement and keeps the fast path, which is the harder of the two and is
written up as the design work in plan item 3. **If a future reading of `849202a` makes the byte
loop look like part of the security fix: it is not.** The confinement is the anchored
`O_NOFOLLOW` destination; what writes into it is unrelated.

### Reading mtime off the source instead of the plan — `be4d2ba` (constrains plan item 4)

Alongside the permissions fix, which kiki is taking, the fork switched the staged copy's mtime
from the planned `mtime` argument to `source.metadata()?.modified()`:

```rust
-            if mtime > 0 { tmp.as_file().set_times(…from_millis(mtime))?; }
+            let metadata = source.metadata()?;
+            if let Ok(modified) = metadata.modified() { tmp.as_file().set_times(…modified)?; }
```

For local→local that is harmless and slightly better. Applied to the remote cases it is not:
the `mtime` argument is what carries the `exact_times` reasoning at
`kikid/src/mirror/execute.rs:~340`. An FTP `LIST` gives minutes for a recent file and only the
day for one older than six months, so the plan deliberately passes a rounded time that the next
run's size-and-time comparison will not read as changed. Reading a real mtime instead
reintroduces that mismatch, and every plain download looks changed on the next run for ever.

### Fixing the temp-dir race at the caller — `028fc76` (constrains plan item 8)

The collision is real in kiki too — that is plan item 8 — but the fork's fix is the wrong shape
to copy. It put the counter in `temp_tree`:

```rust
static NEXT_TREE: AtomicUsize = AtomicUsize::new(0);
let id = NEXT_TREE.fetch_add(1, Ordering::Relaxed);
let dir = std::env::temp_dir().join(format!("path-listing-{}-{id}-{n}", std::process::id()));
```

which fixes one caller and leaves the next one to rediscover it. `kikid/src/listing/tests.rs`
alone has eight other `Scratch::new` tags that are unique only because somebody picked them
carefully, and `Scratch` is used well beyond that file. The counter belongs in `Scratch::new`
(`kikid/src/scratch.rs:17-19`), where it makes every scratch unique by construction.

### SSH certificate authorities, silently — `849202a` (constrains plan item 6)

If the russh upgrade is taken, `check_server_key` starts receiving a `PublicKeyOrCertificate`,
and the fork answers the certificate variant with a bare `Ok(false)`:

```rust
let PublicKeyOrCertificate::PublicKey { key, .. } = offered else {
    return Ok(false); // Certificate authorities are not part of Path's trust model.
};
```

The *policy* is right for kiki as well — kiki pins keys and has no CA trust. What is declined is
the silence. A bare `false` there surfaces through
`kikid/src/locations.rs:267-276` as an ordinary refusal, which reads to the user exactly like a
changed host key: the one error they must not misread, because that is the one that means stop.
The refusal says a certificate was offered and that kiki pins keys.

### Skipping the hostile-filename test when nvim is absent — `849202a` (constrains plan item 2)

The fork's Neovim test opens with

```rust
if !on_path("nvim") { return; }
```

so on a container without Neovim it proves nothing, says nothing, and reports a green tick. A
security test that silently does not run is worse than no test, because it answers the question
"is this covered?" with a yes. Kiki has the pattern for this already —
`kikid/tests/dep_audit.rs:17-23`, which prints "not an audit run, nothing to prove here" — and
CI's container gains `neovim` so the test actually runs there.
