# 01 — Dependencies (0.2.0)

**Status:** planned 2026-09-23, after the survey below. Not started; nothing here is for 0.1.0.

Builds on: `../0.1.0/06-remote-locations.md` (the plugin API and the SFTP plugin as built), `../0.1.0/09-omarchy-integration.md` (the D-Bus plugin), `../0.1.0/25-smb.md` (the GIO plugin), `../0.1.0/11-testing.md` (what has to stay green), `../0.1.0/30-code-health.md` (W6–W8, the other after-the-tag work).

## Goal

Fewer crates, for three reasons in this order: a smaller attack and maintenance surface for a program that opens every file a user points it at; a build that a contributor can do in a minute rather than five; and, in one case, a feature — SFTP through the system `ssh` gives kiki `ssh-agent`, `~/.ssh/config` and every authentication method OpenSSH has, which the 0.1.1 loose-ends list wanted anyway.

The rule for what goes: a dependency is replaced only where the replacement is **less** code to own, or is a program the package already depends on. Nothing is rewritten for the sake of a smaller lock file.

## Where the crates are (2026-09-23, 294 in the lock)

| Package | Direct | Tree | What for |
|---|---|---|---|
| `kiki-plugin-sftp` | `russh`, `russh-sftp`, `tokio` | **193** | an SSH implementation, and the async runtime it needs |
| `kiki-plugin-dbus` | `zbus`, `tokio` | 72 | 188 lines serving `FileManager1` and the portal file chooser |
| `kiki-plugin-ftps` | `suppaftp`, `rustls`, `ring`, `webpki-roots` | 48 | FTP + TLS |
| `kikid` | `libc`, `rustix`, `image`, `png` | 46 | syscalls twice over; image decoding for thumbnails |
| `kiki-plugin-gio` | `gio`, `glib` | 45 | GVfs bindings (SMB) |
| `kiki-plugin-share-mail` | `rustls`, `ring`, `webpki-roots` | 19 | SMTP + TLS, the TOFU fingerprint |
| `share-tailscale`, `sdk`, `json` | `log` | 8 | — |

Trees overlap (`tokio`, `rustls`, `ring`), which is why the total is 294 and not the sum. Done the same day, no behaviour change: `webpki-roots` 0.26 → 1 (the 0.26 line had become a shim over 1.0 and the lock carried both), and an unused `rustls-pki-types` line out of mail's manifest.

## 1. `rustix` out of the daemon (½ day) — first

`kikid` does syscalls through both `libc` and `rustix`. `rustix` is used four times: `process::getuid` twice, one `fs::` call, and the inotify wrapper in `watch.rs`. `libc` does all of it; inotify by hand is `inotify_init1(IN_NONBLOCK | IN_CLOEXEC)`, `inotify_add_watch`, a `read` into a buffer and a walk of `inotify_event` structs (`wd, mask, cookie, len, name[len]`), about forty lines. Gone with it: `rustix` 0.38, `linux-raw-sys`, `errno` and a second copy of `bitflags` — and the second `rustix` beside `zbus`'s 1.1.

- The watch tests (`kikid/tests/watch.rs`, plan 01's 100 ms budget) are the whole safety net; they run unchanged.
- Nothing else changes. `Cargo.toml` loses the `[target.'cfg(target_os = "linux")'.dependencies]` table.

## 2. SFTP through the system `ssh` (4–5 days) — the one with a payoff

**Now**: `russh` speaks SSH itself — key exchange, ciphers, authentication — with `russh-sftp` on top, `tokio` under it, and a thousand lines of plugin around them (`plugins/kiki-plugin-sftp/src/main.rs`): trust-on-first-use against `~/.ssh/known_hosts`, password and key authentication with passphrases, thirty-two reads in flight, and the `find -printf` listing over an exec channel. What it cannot do: use `ssh-agent`, read `~/.ssh/config` (aliases, ProxyJump, per-host keys), or any authentication OpenSSH has that it does not — the top of the 0.1.1 loose ends.

**Then**: OpenSSH does the connecting, and the plugin speaks only the SFTP packet protocol, which is small and ours to implement.

- **One connection, multiplexed.** `ssh -o ControlMaster=yes -o ControlPath=<runtime dir>/kiki-sftp-<id> -o ControlPersist=yes -N host` opens the master; `ssh -S <path> -s host sftp` and `ssh -S <path> host find …` open channels on it without authenticating again. This is how the exec-channel listing keeps working — the same `find` and the same `FastScan` detection, on a second channel of the same connection. Shutdown is `ssh -S <path> -O exit host`.
- **The SFTP protocol** (draft-ietf-secsh-filexfer-02, version 3, what every server speaks): `INIT`/`VERSION`, `OPEN`/`CLOSE`/`READ`/`WRITE`, `STAT`/`LSTAT`/`FSTAT`/`SETSTAT`, `OPENDIR`/`READDIR`, `REMOVE`/`RENAME`/`MKDIR`/`RMDIR`, `REALPATH`, and `STATUS`/`HANDLE`/`DATA`/`NAME`/`ATTRS` back. Length-prefixed packets, big-endian, a request id per packet. About five hundred lines including the pipelining the plugin has today (`READ_IN_FLIGHT`, `READ_CHUNK`, the short-read follow-up), which is a matter of writing thirty-two `READ`s before waiting for the first `DATA`. Nothing async: one thread writes the pipe, one reads it, a map of pending ids between them — the shape `kiki-plugin-sdk` already runs plugins in.
- **Host keys**: OpenSSH owns `known_hosts`. Unknown host: `StrictHostKeyChecking=yes` refuses, the plugin runs `ssh-keyscan -p port host | ssh-keygen -lf -` for the fingerprint and returns the same `Refused`/fingerprint reply the dialog shows today; the user's yes is written with `ssh-keygen`-compatible lines to `known_hosts` (or `KIKI_KNOWN_HOSTS`, as now). Changed key: OpenSSH's own refusal, passed on in the plugin's words. Nothing of `known_hosts` is parsed by kiki any more.
- **Passwords and passphrases**: OpenSSH reads neither from stdin. `SSH_ASKPASS=<the plugin binary itself> SSH_ASKPASS_REQUIRE=force` with the secret in an environment variable the plugin hands its own askpass mode (`kiki-plugin-sftp --askpass` prints it and exits); OpenSSH ≥ 8.4, which every Arch has. The secret never touches the command line or a file. Key files: `-i` from the form's `identityFile` as now, and with none given OpenSSH's own search (agent, then the default files) — which is the feature.
- **The form** (`Describe`) keeps its fields. `auth: "agent"` joins `password` and `key` as a choice, the default when `SSH_AUTH_SOCK` is set.
- **Capabilities** are unchanged: `sizeMtime` both ways, `setMtime` through `SETSTAT`, no trash.
- **Packaging**: `openssh` moves from `optdepends` ("SFTP host key tools") to `depends`.
- **Tests**: `tests/mock_sftp.rs` runs a `russh` server in-process and goes with `russh`. Its replacement is the real thing: `sshd` on a high port with a per-test config, host key and authorized key under a temp dir — CI's rust job installs `openssh` already and the e2e harness (`tests/e2e/servers.py`) starts `sshd` this way today; the plugin's tests do the same in Rust. What they assert stays: every wire case in `mock_sftp.rs` (short reads, a server that caps `READ` at 32 KiB, rename over, the exec listing's three `FastScan` shapes, a hung server), plus new: an agent-only login, a `~/.ssh/config` alias, and a changed host key.
- **Not touched**: the exec listing (`find -printf` / `stat -c` / `stat -f`), the SDK, the daemon, the dialog, the FTPS plugin.

After: the plugin is ~1,200 lines of Rust with **no dependencies** beyond the SDK, and the lock loses about 190 crates.

## 3. D-Bus without `zbus` — not unless the build time bites

`zbus` is well kept and the plugin is 188 lines. Speaking the D-Bus wire protocol ourselves — the auth handshake, `Hello`, `RequestName`, marshalling for the two interfaces' handful of signatures — is about six hundred lines to own for seventy crates to lose. Do it only if a full rebuild is still a nuisance after items 1 and 2; then the design is: one thread, blocking reads on the session bus socket, requests forwarded to kikid over stdout exactly as now. `sd-bus` through `libc` FFI is the other route and trades crates for a C dependency; no.

## 4. Not doing

- **GVfs through the `gio` command** (−45). `gio mount|list|info|copy|cat|save|remove|mkdir` cover the plugin's calls, but mount password prompts come through the CLI's own stdin prompts and structured `IOErrorEnum` errors become string matching. Medium risk, no user-visible gain, and `glib2` stays a runtime dependency anyway.
- **Thumbnails through `ffmpeg`** (−30, `image` and `png`). `ffmpeg` is already required and decodes every still format we list, but a process per thumbnail is ~30–50 ms against ~5 ms in-process, and a thousand photographs on first paint would feel it. The thumber's own process (plan 31 phase 4b) is the isolation that matters.
- **`rustls`/`ring`**: the alternative is OpenSSL, which is more surface, not less. `tokio` where `zbus` needs it. `log`: eleven hundred lines, and how the SDK keeps what a library says about a transfer with the job it was said about (plan 32).

## Order and verification

1. `rustix` (½ day). `cargo test -p kikid`, `make test-e2e`'s `listing_live`; lock file down four.
2. SFTP (4–5 days), on a branch: the protocol module with its tests first, against `sshd`; then the plugin over it; `mock_sftp.rs` cases ported one by one; then `tests/e2e/run.sh --flow remote_transfers mirror_sftp` against the real server, and the by-hand sign-in against the owner's real host that phase 8 did. The old plugin is deleted in the same change, not kept beside the new one.
3. D-Bus: decide after 2, by measuring a cold `cargo build --release` before and after.

Each item is its own commit with the lock-file delta in the message. `make lint`, `cargo test`, `make test-qml` and the e2e flows green before and after each.

## Size

| Item | Days | Crates |
|---|---|---|
| 1 `rustix` | ½ | −4 |
| 2 SFTP over `ssh` | 4–5 | −~190 |
| 3 D-Bus (conditional) | 3 | −~70 |

## Decisions wanted

| | Question | Recommendation |
|---|---|---|
| D1 | `openssh` a hard dependency of the package? | Yes. Omarchy ships it; without it SFTP is not a kind this build speaks, and the alternative is keeping `russh` for the case where it is absent, which is the whole cost. |
| D2 | Keep `auth: "key"` with an explicit file, or agent only? | Keep it. A key file without an agent is the headless-box case, and `-i` costs nothing. |
| D3 | Item 3 at all? | Not now. Revisit with the number from a cold build after item 2. |
