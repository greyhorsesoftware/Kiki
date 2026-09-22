# kiki location plugin API

How `kikid` talks to a location plugin. A plugin is an executable that gives kiki a URI scheme: `sftp://`, `ftps://` and `smb://` ship, and `s3://` or `dav://` would be written the same way. It can be written in any language; the shipped ones are Rust. This document is complete: nothing else is needed to write one — but read "a dropped-in location plugin is ignored" below before starting.

Protocol version: `1`.

## Discovery and lifecycle

- Name: `kiki-plugin-<name>`, executable, in `/usr/lib/kiki/plugins/` or `~/.local/lib/kiki/plugins/` (user directory wins). `Describe` returns `kind`: `location` (then `<name>` is the URI scheme: lowercase ASCII letters, digits, `+`, `-`, `.`), `share` (`<name>` is `share-<id>`), or `service` (a fixed name the daemon uses: `dbus`; its request set is in plan 09).
- **A dropped-in location plugin is ignored.** The kinds a build speaks are compiled in — `plugin::LOCATION_KINDS` in `kikid/src/plugin.rs`, which in a release build is `ftps`, `sftp`, `smb` (and `stub` under the test feature). A binary whose suffix is not on that list is inventoried and listed on the Settings page, and nothing else: it is never spawned for a scheme, never `Describe`d for the Add-location dialog, and `AddLocation` refuses it, saying what kind it was. So an out-of-tree location plugin is an ordinary binary in a directory **and** one line of the daemon — the ABI is stable, the list is not open. Share and service plugins are not kinds and are found by name, so they do drop in.
- **`smb` is `kiki-plugin-gio` installed under that name**: a kind is the suffix of the name the plugin is run by, so `gio` itself is never a kind. The same binary can speak WebDAV and AFP; 0.1.0 installs neither, and a location saved as `dav` by an older build is told "WebDAV is not in this version" rather than `dav`.
- At daemon start, each plugin is spawned once, sent `Describe`, and exited. The result is cached until the plugin file's mtime changes.
- The daemon spawns the plugin on the first request for its scheme and keeps one process per scheme. All locations of that scheme share it; the plugin keeps a session per `(location, role)`.
- Environment: `KIKI_PLUGIN_PROTOCOL=1`, `KIKI_PLUGIN_SCHEME=<scheme>`, plus the user's environment. No arguments.
- `stdin` and `stdout` carry the protocol. `stderr` is logged by the daemon at debug level; write diagnostics there, never to stdout.
- Idle exit: after 5 minutes with no requests the daemon sends `Shutdown` and waits 5 s before killing. A plugin may also exit on its own after replying to `Shutdown`; it must not exit with requests outstanding.
- Crash: if the process dies, every outstanding request fails with `Plugin` and the next request spawns it again. Sessions are gone; the daemon re-`Connect`s as needed.
- **Process group**: every child is spawned into a process group of its own, and the daemon kills the *group*, not the process. A plugin that starts a helper — gio reaching `gvfsd`, the thumbnailer running `ffmpeg` — leaves that helper holding the write end of `stdout`; a host that killed only the child would then wait on a pipe that never closes, which is a hang with no visible cause. A plugin does not have to do anything for this, but it should not put itself in another group.
- **Not every child is a location plugin.** `kiki-thumber` (see `API-DAEMON.md`) speaks this framing and nothing else: no `Describe`, no `Connect`, no scheme of its own. The framing and the request/reply rules below are the daemon's one way of running a child; the discovery, sessions and `LOCATION_KINDS` above apply only to plugins that are locations.

## Framing

Identical to the socket protocol: `u32` little-endian payload length, `u8` type (`0x00` JSON, `0x01` binary), payload. Binary frames belong to the most recent `Read` or `Write` in flight on the pipe; a zero-length binary frame ends the stream. Maximum JSON frame 16 MiB, binary frame 1 MiB.

Requests from the daemon carry `id`; the plugin replies `{ "id", "ok": {…} }` or `{ "id", "err": { "code", "message", "field"? } }`. Streaming replies (`Scan`, `Read`) send their stream and then the final reply. Requests are pipelined and **served concurrently**: the SDK reads frames on the calling thread, runs each request on its own worker (up to 8), routes the binary frames of the one `Write` in progress to that handler, and serialises `Read`/`Thumb` so binary frames never interleave; JSON frames of different requests may interleave freely (each carries its `id`). Handlers take `&self` and keep sessions behind their own locks; `kiki_plugin_sdk::cancelled()` tells a loop that its request was cancelled.

Error codes the plugin may return: `NotFound`, `Denied`, `Exists`, `NotEmpty`, `Unsupported`, `Cancelled`, `Auth` (credentials rejected), `Network` (connection failed or dropped), `Io`, `Invalid` (with `field`, for `Validate` and `Connect`).

## Types

- **Path**: string, the path inside the location as the remote sees it, `/`-separated, absolute. The daemon never sends a URI to a plugin; it resolves the location and hands the plugin the path.
- **Kind**: `"dir" | "file" | "link" | "other"`.
- **Meta**: `{ "size": u64, "mtime": u64 (ms, 0 unknown), "mode": u32 | null, "owner": string | null, "group": string | null, "digest": { "kind": string, "hex": string } | null }`.
- **Field**: `{ "key", "label", "kind": "text" | "password" | "path" | "port" | "select" | "file" | "browse", "required": bool, "default": string | null, "options": [string] | null, "group": string | null, "page": string | null, "help": string | null }`.
  - **`group`** makes a tab: fields sharing a group are alternatives, only the chosen tab's fields are sent, and which tab was chosen arrives in the config as `auth` (the group's name, lower-cased) — the SFTP plugin's Password / Key.
  - **`page`** makes a section: pages are shown one at a time and **all** of them are sent, so they are not alternatives. Every shipped location plugin puts its host and credentials on `Connection` and its two path fields on `Locations`. A field with no page is on the first.
- **Config**: `{ key: string }`, the non-secret fields. **Secrets**: `{ key: string }`, the fields named in `secretFields`. Secrets arrive only in `Connect` and must never be written to disk, to stderr or to a log line.
- **Role**: `"browse"`, or `"job-<id>"` for a job's own session. Browsing has one session per location; a job that transfers, deletes, shares or mirrors gets sessions of its own, opened when it first touches the server and closed when the job ends, however it ends — so a cancelled upload never disturbs the pane. There may be several at once. Every request names the session it is for, and the SDK hands it to the handler as `sdk::current_role()`; a plugin that keys its sessions by location alone will hand one job's connection to another's `Disconnect`. `mkdir` and `rename` are one round trip and stay on the browser's session.
- **A cached session is only the server it was made for.** Both shipped plugins used to hand back a session by `(location, role)` without looking at the config, so a location removed and another added under the same name inherited the connection to the old machine — with nothing on screen to say so. Compare a pure identity (host, the port it will dial, username, auth tab and keys, TLS mode, pinned fingerprint, the secret it will send) before reusing one, and reconnect when it differs.

## Messages

### Description and validation

| Request | Fields | Reply |
|---|---|---|
| `Describe` | | `{ "kind": "location", "scheme", "displayName", "version": string, "form": [Field], "defaults": Config, "secretFields": [string], "detector": { "upload": "sizeMtime" \| "sizeOnly" \| "digest", "download": … }, "available": bool, "unavailableReason": string, "features": { "setMtime": bool, "mode": bool, "realDirs": bool, "digestKind": string \| null, "separator": string, "metaInScan": bool, "pipelining": bool, "partialRead": bool } }` |
| `Validate` | `config` | `{}` or `Invalid` with `field` |
| `Browse` | `field`, `config`, `secrets` | `{ "options": [{ "value", "label" }] }` choices for a `browse` field given the form so far (optional; `Unsupported` when the plugin has none) |

`detector` tells the mirror engine which change detector to use by direction (`upload` = local master). `metaInScan: true` promises that `Scan` entries carry `meta`.

**`available: false` with an `unavailableReason`** says the plugin cannot work on this machine — a missing daemon or library — and the Add-location dialog shows the reason in place of the form and will not add. It is the whole answer a user gets, so write a sentence they can act on ("install gvfs-dnssd"), not a code.

### Sessions

| Request | Fields | Reply |
|---|---|---|
| `Connect` | `location: string`, `role`, `config`, `secrets` | `{ "fingerprint": string \| null, "banner": string \| null }` or `Auth` / `Network` / `Invalid` |
| `Disconnect` | `location`, `role` | `{}` |
| `Capabilities` | `location` | `{ "trash": bool, "setMtime": bool, "mode": bool, "realDirs": bool, "digestKind": string \| null, "separator": string, "fastScan": string \| null }` (`fastScan` names the accelerated listing path the plugin probed: `"gnu"` for SSH exec with GNU find, `"posix"` for `find -exec stat`, `"none"`; informational; `partialRead: false` tells the daemon the backend cannot serve byte ranges, plan 17) |
| `Ping` | | `{}` |
| `Shutdown` | | `{}` then exit |

Connect is idempotent: a second `Connect` for an open session returns the same reply. If the server's host key or certificate is unknown, return `Invalid` with `field: "fingerprint"` and the fingerprint in `message`; the daemon asks the user and retries `Connect` with `config.trustedFingerprint` set.

### Listing

| Request | Fields | Stream | Reply |
|---|---|---|---|
| `Scan` | `location`, `path`, `id` | JSON frames `{ "id", "entries": [{ "name", "kind", "meta"? }] }` in chunks of up to 1024 | `{ "n": u64 }` |
| `Stat` | `location`, `path` | | `Meta` |

`Scan` lists one directory, not recursively, unless `recursive: true` is passed, in which case entries carry `rel` (path relative to `path`) and the plugin may stream the whole tree in one go where its protocol allows (SFTP exec acceleration); a plugin that cannot recurse returns `Unsupported` and the daemon walks directory by directory. Names are as the remote returns them; the daemon skips `.` and `..` if present. Symbolic links are reported as `link` and never followed. If the protocol returns attributes with the listing (SFTP, FTPS `MLSD`), include `meta` on every entry and set `metaInScan` in `Describe`; the daemon then never calls `Stat` for windows or mirror scans.

### Files

| Request | Fields | Stream | Reply |
|---|---|---|---|
| `Read` | `location`, `path`, `id`, `offset`: u64 (default 0) | plugin sends binary frames, then a zero-length frame | `{ "bytes": u64 }` |
| `Write` | `location`, `path`, `id`, `size`: u64 \| null, `mode`: u32 \| null, `mtime`: u64 \| null | daemon sends binary frames, then a zero-length frame | `{ "bytes": u64 }` |
| `Thumb` | `location`, `path`, `id` | plugin sends binary frames (a JPEG or PNG), then a zero-length frame | `{ "bytes": u64 }` or `Unsupported` (optional; protocols with native thumbnails, plan 17) |
| `Mkdir` | `location`, `path` | | `{}` |
| `Rename` | `location`, `from`, `to` | | `{}` (same location only) |
| `Delete` | `location`, `path` | | `{}` (file or empty directory) |
| `SetMtime` | `location`, `path`, `mtime`: u64 ms | | `{}` or `Unsupported` |
| `Chmod` | `location`, `path`, `mode`: u32 | | `{}` or `Unsupported` |
| `Cancel` | `target` | | `{}`; the request with id `target` stops as soon as its loop notices and replies `Cancelled` (already-sent frames are discarded by the daemon) |

Streams: only one `Read` or `Write` stream is active per pipe at a time; the daemon serialises them and uses the job's own session for transfers so browsing requests (`Scan`, `Stat`) stay responsive on the `browse` session. `Write` creates or truncates. A plugin should keep several protocol requests in flight for `Read` and `Write` when the protocol allows (`pipelining: true`); SFTP throughput depends on it.

### The log

A plugin sends its lines as a **notification** — an object with `event` and no `id`, which may arrive between any two frames, a binary stream's included:

```
{ "event": "Log", "level": "debug", "target": "russh::client", "message": "…", "role": "job-41" }
```

`role` is the session the sending thread is serving, and it is how the daemon files the line: a job's sessions are its own, so the line is that job's, read back with `JobLog`. A line from a library's background thread — a keepalive, a rekey — serves nobody, carries an empty role, and goes to every job with a session open on this plugin just then. A `browse` line belongs to no job and goes to the location's own ring, which is what "Connection Log…" shows for a location that will not connect (`LocationLog`).

- **Only when asked.** The daemon sets `KIKI_PLUGIN_LOG=1` when it spawns a plugin. A host that did not ask gets no `Log` frames at all — a strict reader must not find an event in the middle of a stream it thought it knew, and the FTPS plugin's own test is such a reader.
- **Level.** `Debug` while a job's session is open on this plugin, `Info` otherwise, and **never `Trace`**. Libraries whose debug output is the wire rather than the story — `russh`, `russh_sftp`, `rustls`, `tokio`, `mio` — are heard from `Info` up: russh wrote 380 lines for six small files where the SDK's own account of the same transfer is 26.
- **A plugin's own lines** are what make a log readable: `connect` and `disconnect` with the role, `write path (n bytes)`, `read`, `mkdir`, `delete`, `rename a -> b`, and any refusal as a warning with its code. Listing and stat are left out — a browser does thousands.
- **Secrets never go in, and are taken out here rather than in the viewer**: a log is something people paste into bug reports, and by then it has been read. The SDK redacts what follows the FTP `PASS ` command, and what follows `password`, `passphrase`, `authorization`, `secret` or `token` **when a `:` or `=` follows it** — to the end of the line. A sentence that merely mentions one ("password authentication failed for gideon") is left alone: that is the line somebody needs. Never log a secret, a key, a token, an `Authorization` header or a URL with credentials in it, and never rely on the redactor for a shape it has not been taught — it is the last guard, not the first.

The SDK installs all of this for a Rust plugin: `log::info!` and friends go through it, the gio plugin routes GLib's writer into the same sink, and `sdk::log` re-exports the facade.

## Session flow

```
daemon → Describe                                   (at daemon start; process exits after)
…
daemon → Connect {location:"homelab", role:"browse", config, secrets}
plugin ← {ok:{fingerprint:"SHA256:…", banner:null}}
daemon → Scan {location:"homelab", path:"/srv/kiki", id:5}
plugin ← {id:5, entries:[{name:"design",kind:"dir",meta:{…}}, {name:"README.md",kind:"file",meta:{…}}]}
plugin ← {id:5, ok:{n:2}}
daemon → Read {location:"homelab", path:"/srv/kiki/README.md", id:6}
plugin ← 0x01 <1126 bytes>
plugin ← 0x01 <0 bytes>
plugin ← {id:6, ok:{bytes:1126}}
…
daemon → Shutdown
plugin ← {id:…, ok:{}}                               (then exits)
```

## Writing a plugin

Minimal loop, any language:

```
read frame
  if JSON: dispatch on "type"; reply with the same "id"
  if binary: append to the current Write stream; zero-length ends it
write frames with a single writer (serialise stdout across threads)
```

Rules that keep plugins interchangeable:

- Never block the pipe on a slow session: answer `Ping` and browse-session requests while a `job` transfer is running.
- Report errors with the codes above; put server text in `message`.
- Treat `config` values as strings and validate them yourself in `Validate`; the form is rendered by kiki from `Describe`, so a new field needs no UI work.
- Keep secrets in memory only; the daemon owns the keyring, and nothing about them goes to stderr or into a `Log` line.
- Say what you are doing in `Log` lines, which the user can read against the job that caused them; stderr is captured too, at debug level, with your scheme as the prefix.

A stub plugin (`kiki-plugin-stub`, in the test tree) implements this protocol over an in-memory tree with a two-field form and is what the daemon's contract tests run against.

## Share plugins

A second plugin kind, `kiki-plugin-share-<id>` in the same plugin directory, uses the same framing, lifecycle, error codes, `Field` type and `Log` notification, with its own request set: `Describe`, `Configure`, `Targets`, `Share` (streamed `Progress`), `Cancel`, `Ping`, `Shutdown`. Files always arrive as local `file://` URIs; the daemon fetches remote and device files and compresses folders beforehand. The full contract is in `docs/0.1.0/18-share.md`.

`Describe` answers `{ "kind": "share", "id", "name", "icon", "version", "accepts": { "files", "folders", "multiple", "maxBytes" }, "targets": "list" | "search" | "none", "form": [Field], "secretFields": [string], "compose": [Field], "requires": [string] }`.

- **`requires`** names the programs the plugin cannot work without (`tailscale`). The daemon looks for each of them on `PATH` **every time it lists the plugins**, not when the plugin described itself, and adds **`unavailable`** — a sentence saying what is missing — to the entry it hands the shell. The menu then shows the way of sending dimmed and says why, instead of offering something that fails; and installing the program brings it to life without restarting anything.
- There is no **`defaultEnabled`**: a plugin that is installed is on until the user switches it off, because an installed way of sending that the menu hides is a way of sending nobody finds. The field existed for LocalSend, which shipped off, and went with it on 2026-09-21.
- Shipped: `share-mail` (Thunderbird's composer) and `share-tailscale` (Taildrop).

## Compatibility rules

- The daemon ignores unknown fields in replies; plugins must ignore unknown fields in requests.
- New request types may be added within a version; a plugin returns `Unsupported` for any it does not know.
- Removing a field or changing its type bumps the protocol version, which the daemon checks in `Describe`'s `version` prefix (`1.x`).
