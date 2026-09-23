# 06 — Remote locations

**Status:** built and tested against real `sshd` and FTPS; agent auth not in 0.1.0.

Builds on: `01-daemon-and-listing.md` (the listing source trait), `04-operations-and-undo.md` (transfers are jobs).

Mockups: `AddLocationSFTP.dc.html`, `AddLocationFTPS.dc.html`; the Locations section in every window.

## Goal

Remote location types are plugin processes. kikid ships three in 0.1.0 — `kiki-plugin-sftp`, `kiki-plugin-ftps` and the GIO plugin installed as `kiki-plugin-smb` — added from the sidebar's `+` button, with credentials in the Omarchy keyring. Each location has a remote URI and a local URI; the local URI is what Side by Side (plan 07) and mirror (plan 08) open on the left. Adding a third protocol later (SMB in plan 25; S3 after) means shipping a binary that speaks the protocol below, in any language, without touching the daemon or the UI. **WebDAV is not in 0.1.0** (owner, 2026-09-21): the gio plugin can speak it and is installed under the `smb` name only, so the `dav` kind is never offered — see `25-smb.md`.

## Plugin processes

A plugin is an executable named `kiki-plugin-<scheme>` found in `/usr/lib/kiki/plugins/` or `~/.local/lib/kiki/plugins/`. The daemon spawns it on the first request for its scheme, talks to it over its stdin and stdout with the same framing as the socket (length-prefixed JSON, binary frames for bytes), and lets it exit after 5 minutes idle; a plugin that dies is respawned on the next request and the failed request is reported, never retried silently. One process per scheme serves every location of that scheme; the plugin multiplexes sessions by location name.

Messages the plugin must answer:

```
Describe -> { scheme, display_name, form: [Field], defaults, secret_fields, detector: { upload, download } }
Validate { config } -> Ok | { field, message }
Connect { location, config, secrets } -> Ok | { field?, message }      opens or reuses a session
Disconnect { location }
Capabilities { location } -> { trash, set_mtime, mode, real_dirs, digest_kind, separator }
Scan { location, path } -> streamed { entries: [(name, kind)], done }   phase 1
Stat { location, path } -> Meta
Read { location, path } -> binary frames … done
Write { location, path } <- binary frames … done
Mkdir, Rename, Delete, SetMtime, Chmod { location, … }
Ping
```

- `Field` is `{ key, label, kind: Text | Password | Path | Port | Select(options) | File, required, default, group }`. The Add-location dialog renders it generically; the mockups are that rendering for the two shipped plugins.
- The daemon's `vfs::remote::RemoteDir` implements the plan-01 listing trait over a plugin session; listings from plugins go into the same string pool and window machinery as local ones, so a remote pane is indistinguishable to the UI. Operations are not the same trait over a pipe: they go through `locations::Session` and `plugin.rs`, which is what `transfer.rs` and the mirror engine call.
- The daemon runs `Describe` on every plugin at startup (spawn, describe, exit) and caches the result, so the dialog's tabs and the resolver's schemes are known without keeping plugins alive.
- `Read` and `Write` stream through the daemon. ~~Between a plugin pipe and a local file the daemon uses `splice`, so file bytes move pipe-to-file inside the kernel without a user-space copy; only the frame headers are parsed.~~ **Amended 2026-09-21:** there is no `splice(2)` anywhere in the daemon — a plugin's binary frames are read and written like any other frame, and the measured throughput (73 MB/s up and 29–35 down over SFTP on loopback, 114/139 over FTPS with session reuse) is bounded by the protocol, not by that copy. A transfer between two plugins is a `Read` from one fed into a `Write` on the other through a bounded in-memory channel, with the daemon counting bytes for progress.
- `Scan` returns `Meta` inline when the protocol provides it for free (SFTP readdir attributes, FTPS `MLSD`), so remote listings arrive fully enriched in phase 1 and mirror scans make no per-file requests.
- Plugins may use any runtime and any crates. The shipped two are Rust with tokio, `russh` and `suppaftp`; that graph stays in their binaries.

## Shipped plugins

**SFTP** (`russh` + `russh-sftp`): fields Name, Host, Port (22), Username, then **two tabs, one way in** — Password, or Keys (ticked from the private keys found in `~/.ssh`) with a Key passphrase; the form sends only the chosen tab's fields, and the secrets go to the keyring. Remote path and Local path are on the Locations page. `Read` keeps 16 chunk requests of 256 KiB in flight on a second SFTP channel opened for that purpose (SFTP throughput is bounded by round trips, not bandwidth, with one outstanding request); `Write` uses the crate's own concurrent writes; readdir attributes fill `Meta`.

*Exec acceleration.* SFTP `READDIR` returns roughly a hundred entries per round trip, so a large directory costs many sequential round trips and a whole tree costs one per directory. When the server also allows command execution on the same SSH connection, the plugin lists through a single exec channel instead:

- **Probe**, once per `Connect` and cached with the session: open an exec channel and run `command -v find >/dev/null 2>&1 && find --version 2>/dev/null | head -1`; if that does not say GNU, a second probe checks for `find` plus a formattable `stat` (`stat -c '%F' /`, else `stat -f '%HT' /`). Outcomes: `gnu` (GNU findutils present), `posix` (a `find` without `-printf` but a usable coreutils-style `stat -c` or BSD-style `stat -f`), or `none` (exec refused, restricted shell, `ForceCommand internal-sftp`, no `find`). The probe has a 3 s timeout; any failure means `none`. The result is reported in `Capabilities` as `fastScan: "gnu" | "posix" | "none"` so the UI can show it in the location's tooltip.
- **Single directory** (`Scan`): with `gnu`, run `find <path> -mindepth 1 -maxdepth 1 -printf '%y\0%Y\0%s\0%T@\0%m\0%u\0%g\0%f\0'` and parse NUL-separated records: type, link target type, size, mtime with fraction, octal mode, owner, group, name. Filenames with spaces or newlines are safe because every field is NUL-terminated. With `posix`, run `find <path> -mindepth 1 -maxdepth 1 -exec sh -c 'for f; do stat -c "%F|%s|%Y|%a|%U|%G" "$f"; printf "%s\0" "$f"; done' sh {} +` (the `stat -f "%HT|%z|%m|%OLp|%Su|%Sg"` form on BSD) and parse one `stat` line plus a NUL-terminated path per entry, so names with newlines or quotes survive there too; with `none`, use SFTP `READDIR`. Paths are single-quote escaped before being placed in the command; nothing from a filename is ever interpolated unquoted.
- **Whole tree** (the mirror scan in plan 08, "This location" search in plan 12, size totals): the same command without `-maxdepth`, plus `%P` for the path relative to the root, streamed and parsed as it arrives. One round trip for the entire tree; a 100,000-file tree lists in a few seconds instead of minutes.
- **Fallback** is per call: if the exec channel fails mid-stream (connection limit, killed process, exit status other than 0 or 1), the plugin marks `fastScan` as `none` for the rest of the session and repeats that call over SFTP. Entries from an exec stream are held back until the command finishes, so a partial stream is discarded, never merged. GNU find's exit status 1 (an unreadable entry somewhere) with a cleanly parsed stream is still a listing. Exec is refused cleanly: a `ChannelMsg::Failure` closes the channel and the probe answers `none` (3 s timeout).
- **Consistency**: both paths produce the same `Entry` fields; the contract tests run every listing test twice, once with exec forced off, and diff the results.
- Exec is used for listing only. Reads, writes, renames, deletes and permission changes stay on SFTP, where semantics are well defined. ~~Agent auth is tried first when no identity file is given.~~ **Struck 2026-09-21 — not in 0.1.0**: sign-in is a password, or keys (every private key found in `~/.ssh`, or the ones the location names, with the passphrase in the keyring), plus keyboard-interactive answered with the password; `ssh-agent` is never asked and `~/.ssh/config` is never read, so a host alias, a `ProxyJump` or an `IdentityFile` written there means nothing here. `~/.ssh/known_hosts` **is** read, for the trust step. Symlinks are listed as links and never followed.

**FTPS** (`suppaftp` with rustls): fields Name, Host, Port (21), Username, Password, Encryption (Explicit TLS, the default; Implicit TLS on 990), Remote path, Local path. Passive mode only. Certificate errors surface in the Connect step with the fingerprint so the user can pin it per location.

## Locations, secrets, sessions

**Location model** (`~/.config/kiki/locations.toml`): `name` (unique, the URI authority), `plugin`, `remote_uri` (`sftp://homelab/srv/kiki`), `local_uri` (`file:///home/david/Projects/kiki`), `config` (the plugin's non-secret fields). No secrets.

**Keyring**: the daemon talks to `org.freedesktop.secrets` with its small D-Bus client (CORE.md) and hands secrets to the plugin only in `Connect`. Attributes `{app: kiki, location: <id>, field: <key>}`; one secret per secret field. Adding a location writes secrets first; removing a location deletes them.

**Connections**: the plugin holds one browsing session per location. For a job (plan 04) the daemon asks the plugin for a second session (`Connect { location, role: job }`), so a transfer never shares a channel with the browser and a cancelled job cannot break the next listing; the plugin closes it when the job ends. Sessions idle out after 5 minutes. **Every request names its session** (`Session::req`, `sdk::current_role()`), so the role cannot be forgotten, and a cached session is compared field for field against the config before it is handed back — a location removed and another added under the same name used to inherit the first one's connection, in both plugins.

**Connected, and letting go** (2026-09-21). `Locations` answers `connected` per location — whether a browse session is alive — and the daemon sends `LocationsChanged` when one comes or goes, so the sidebar row wears a **green dot** that follows a connect and a Disconnect. **Disconnect** is in two places and is one function (`Shell.disconnectLocation`): the toolbar's broken-chain button beside Mirror, and the sidebar row's menu item. It sends `Disconnect`, **moves any pane standing on that server to the location's local folder, or home** — a pane left on a server that has gone can only say so — and, Side by Side being that server and its folder, goes back to one pane. `locations::remove` disconnects and clears the keyring in the right order, and `disconnect` takes sessions out of the map before it tells the plugin: holding the lock across that round trip deadlocked the whole daemon, and the sidebar's menu item was one click from it (`kikid/tests/location_lifecycle.rs`).

**Shipped kinds**: which protocols a kiki speaks is decided when it is built, not configured at run time. `plugin::LOCATION_KINDS` in `kikid/src/plugin.rs` lists them — `ftps`, `sftp` and `smb` — and discovery (`available`), `Describe`, the Add-location dialog and `AddLocation` all go through it, so a plugin binary that turns up in the plugin directory anyway is never spawned and `AddLocation` answers `Unsupported`. The workspace's members are that set, so `cargo build --release` and the PKGBUILD produce exactly those plugins; the GIO plugin's other kinds (`dav`, `afp`) are one binary under other names — a kind is the name it is installed under — so those two are an install line away and neither is in 0.1.0. Adding one back is two edits: the const and the PKGBUILD's install list. The device plugins (`mtp`, `ptp`, `afc`) left the tree on 2026-09-23 and are in git history (plan 17). The stub plugin the contract test drives is a location kind too, enabled by kikid's `stub` feature, which only its own dev-dependency turns on.

**Dialog**: one tile per shipped plugin; fields from `form()`. `Ctrl+Shift+L` opens it, as does the `+` in the sidebar. Connect runs `validate` then `connect` before saving; errors show inline on the field the plugin names. Editing a location reopens the same dialog.

**Operations on remote paths** reuse plan 04's jobs: copy and move between any two backends stream through the daemon; rename and mkdir map directly; delete is a confirmed, non-undoable delete because there is no remote trash (the journal records nothing for it).

**Protocol** (UI ↔ daemon): `Plugins -> [Describe results]`, `Locations -> [Location]`, `AddLocation { location, secrets } -> Ok | { field, message }` (validates, connects, writes the keyring then the file), `UpdateLocation`, `RemoveLocation { name }`, `TestLocation { location, secrets }`, `LocationsChanged` event; job ops carry URIs, which the daemon resolves to a plugin by scheme and authority. **IPC added**: `addLocation()` opens the dialog; opening a location is `open(uri)` from plan 02.

## Verification

- Mock SFTP server (`plugins/kiki-plugin-sftp/tests/mock_sftp.rs`, runs anywhere `cargo test` does, no `sshd`): an in-process russh server with an in-memory tree whose exec channel emulates GNU find, `stat -c`, `stat -f`, no `find`, and a refused channel. Asserts: `gnu` and both `posix` flavours list identically to `READDIR` (names with a newline, a quote and a space; symlinks as links; recursive `rel` paths) with zero `READDIR` calls; a stream killed mid-way falls back to `READDIR` with no duplicate entries and no further exec attempts that session; exit status 1 is still a listing; a 4 MiB read is byte-exact with at least 8 READ requests outstanding on the wire (counted at the SSH layer before the SFTP handler sees them) and resumes from an offset; write, stat, mkdir, rename, chmod, set-mtime and delete round-trip; a wrong password is an `Auth` error.
- Plugin contract tests (`kikid/tests/plugin_contract.rs`): one suite drives every plugin binary over its pipe (`Describe` is well-formed, `Validate` rejects a bad port and an empty host, `Connect`, then listing and operations through the daemon's own path). A plugin killed mid-`Read` is a test of its own (`plugin_killed.rs`): the job **fails** naming the file and saying `plugin exited`, leaves no short file and no part file, and the next request gets a new process that copies the same file whole.
- Against a local `sshd` and `vsftpd` with TLS: list, stat, mkdir, rename, delete, upload, download, `set_mtime` on SFTP, all through the plan-01 trait tests.
- SFTP exec acceleration: the probe reports `gnu` on the test `sshd`, `none` on a second `sshd` configured with `ForceCommand internal-sftp`; a 10,000-entry directory lists in one round trip with `gnu` and identically (diffed) over `READDIR`; a filename containing a newline, a quote and a space round-trips through both paths; killing the exec process mid-stream falls back to SFTP with a complete listing and no duplicate entries.
- `secret-tool lookup app kiki location <id>` returns the password; `grep -r` over `~/.config/kiki` finds none.
- Cancelling an upload mid-transfer leaves the browser session usable (next listing succeeds).
- Dropping a stub `kiki-plugin-stub` binary with a two-field form into the plugin directory makes a third tab appear in the dialog with no daemon or front-end change.
