# 02 — `rustix` out of the daemon (0.1.1)

**Done 2026-09-24.** The four call sites now go through `libc` (`getuid` twice in `openin.rs`,
`statx` in `vfs/local.rs` with the same flags and mask, `inotify_init1`/`inotify_add_watch`/
`inotify_rm_watch` in `watch.rs`; the event read was already a `libc::read` walked by hand).
Out of the lock: `rustix` 0.38.44 and `linux-raw-sys` 0.4.15 — two crates, not four: `errno` and
`bitflags` 2 stay because `zbus`'s `rustix` 1.1 still wants them, so the lock now carries one
`rustix` instead of two. Bench on the same day, `taskset -c 0-3`, median of three warm runs each:
`flat200k.enrich_ms` 64.8 → 61.6, `rescan_ms` 119.9 → 133.3 (spread 116–143 on both sides),
`flat10k.enrich_ms` 2.43 → 2.72 — noise, as the plan expected. `make lint` clean; `cargo test -p
kikid` green, `tests/watch.rs` three times over.

**Status:** planned 2026-09-24, moved here from the 0.2.0 dependencies plan (owner: small enough for 0.1.1).

## Goal

`kikid` makes its system calls through two crates, `libc` and `rustix`. `rustix` is used in four
places; `libc` — already a dependency — does all of them. Removing `rustix` takes four crates out
of the daemon's tree (`rustix` 0.38, `linux-raw-sys`, `errno`, a second `bitflags`) and the
second copy of `rustix` that sits beside `zbus`'s 1.1 in the lock. Half a day, no behaviour
change, and the existing tests are the whole net.

## The four uses

| Where | What | With `libc` |
|---|---|---|
| `openin.rs:132`, `:144` | `rustix::process::getuid().as_raw()` | `libc::getuid()` (unsafe, cannot fail) |
| `vfs/local.rs:146` | `statx` with `AT_SYMLINK_NOFOLLOW \| AT_STATX_DONT_SYNC` and a mask of size, mtime, atime, mode, type, uid, gid — **the stat pool's hot path**, every row of every listing | `libc::statx(dirfd, name, flags, mask, &mut st)` — the same syscall through glibc's wrapper (glibc ≥ 2.28; Omarchy has 2.4x). The `struct statx` fields are the same names (`stx_size`, `stx_mtime.tv_sec`…). The mask and flag constants are `libc::STATX_*` and `libc::AT_*`. |
| `watch.rs:13` | `inotify::inotify_init(CLOEXEC)`, `inotify_add_watch(fd, path, flags)`, `inotify_remove_watch`, and the read of events into `inotify_event` records | `libc::inotify_init1(IN_CLOEXEC \| IN_NONBLOCK)`, `libc::inotify_add_watch(fd, c_path, IN_CREATE \| IN_DELETE \| IN_MOVED_TO \| IN_MOVED_FROM \| IN_MODIFY \| …)`, `libc::inotify_rm_watch`; the read is `libc::read` into a `[u8; 64 KiB]` buffer walked record by record — `wd, mask, cookie, len` then `len` bytes of name, NUL-padded — about forty lines, in one `unsafe` block with the struct layout written down beside it. The fd is kept as an `OwnedFd` (`FromRawFd`) so it still closes itself. |

Nothing else: `rustix` is not in the plugins or the SDK.

## Rules

- The stat path is measured. Run `taskset -c 0-3 kikid bench run` before and after, against a fresh
  build of the previous commit on the same day; `flat200k.enrich_ms` and `rescan_ms` must not move — it is the same syscall with the
  same flags, so they should not, and a difference means the mask or the flags are not the same.
- `statx` needs a glibc that wraps it (2.28, 2018). Omarchy's is far past that; the PKGBUILD's
  `depends` says nothing about glibc and need not. The non-Linux fallback (`#[cfg(not(target_os
  = "linux"))]`, `lstat`) stays as it is.
- One `unsafe` block per call site, each with a comment saying what the invariant is (a
  NUL-terminated path, a buffer the kernel writes whole records into, a mask the kernel
  honours). No new abstraction: this is four call sites, not a syscall layer.
- `kikid/Cargo.toml` loses the `[target.'cfg(target_os = "linux")'.dependencies]` table.

## Tests

The ones there are: `kikid/tests/watch.rs` (a watched folder tells every window what changed,
within its 100 ms budget; the watch is given up last), the listing tests that go through
`stat_child`, `local_ops`, and `make test-e2e`'s `listing_live`. `cargo test` and the QML suite
green before and after; the lock file's diff in the commit message: four crates fewer.

## Size

Half a day.
