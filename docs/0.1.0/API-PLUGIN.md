# kiki location plugin API

How `kikid` talks to a location plugin. A plugin is an executable that gives kiki a new URI scheme (`sftp://`, `ftps://`, later `s3://`, `dav://`, `smb://`). It can be written in any language; the shipped ones are Rust. This document is complete: nothing else is needed to write one.

Protocol version: `1`.

## Discovery and lifecycle

- Name: `kiki-plugin-<name>`, executable, in `/usr/lib/kiki/plugins/` or `~/.local/lib/kiki/plugins/` (user directory wins). `Describe` returns `kind`: `location` (then `<name>` is the URI scheme: lowercase ASCII letters, digits, `+`, `-`, `.`), `share` (`<name>` is `share-<id>`), or `service` (a fixed name the daemon uses: `dbus`, `highlight`; their request sets are documented in plans 09, 13 and 19).
- At daemon start, each plugin is spawned once, sent `Describe`, and exited. The result is cached until the plugin file's mtime changes.
- The daemon spawns the plugin on the first request for its scheme and keeps one process per scheme. All locations of that scheme share it; the plugin keeps a session per `(location, role)`.
- Environment: `KIKI_PLUGIN_PROTOCOL=1`, `KIKI_PLUGIN_SCHEME=<scheme>`, plus the user's environment. No arguments.
- `stdin` and `stdout` carry the protocol. `stderr` is logged by the daemon at debug level; write diagnostics there, never to stdout.
- Idle exit: after 5 minutes with no requests the daemon sends `Shutdown` and waits 5 s before killing. A plugin may also exit on its own after replying to `Shutdown`; it must not exit with requests outstanding.
- Crash: if the process dies, every outstanding request fails with `Plugin` and the next request spawns it again. Sessions are gone; the daemon re-`Connect`s as needed.

## Framing

Identical to the socket protocol: `u32` little-endian payload length, `u8` type (`0x00` JSON, `0x01` binary), payload. Binary frames belong to the most recent `Read` or `Write` in flight on the pipe; a zero-length binary frame ends the stream. Maximum JSON frame 16 MiB, binary frame 1 MiB.

Requests from the daemon carry `id`; the plugin replies `{ "id", "ok": {…} }` or `{ "id", "err": { "code", "message", "field"? } }`. Streaming replies (`Scan`, `Read`) send their stream and then the final reply. Requests may be pipelined; the plugin may answer out of order except that binary frames must follow their own request's order.

Error codes the plugin may return: `NotFound`, `Denied`, `Exists`, `NotEmpty`, `Unsupported`, `Cancelled`, `Auth` (credentials rejected), `Network` (connection failed or dropped), `Io`, `Invalid` (with `field`, for `Validate` and `Connect`).

## Types

- **Path**: string, the path inside the location as the remote sees it, `/`-separated, absolute. The daemon never sends a URI to a plugin; it resolves the location and hands the plugin the path.
- **Kind**: `"dir" | "file" | "link" | "other"`.
- **Meta**: `{ "size": u64, "mtime": u64 (ms, 0 unknown), "mode": u32 | null, "owner": string | null, "group": string | null, "digest": { "kind": string, "hex": string } | null }`.
- **Field**: `{ "key", "label", "kind": "text" | "password" | "path" | "port" | "select" | "file", "required": bool, "default": string | null, "options": [string] | null, "group": string | null, "help": string | null }`.
- **Config**: `{ key: string }`, the non-secret fields. **Secrets**: `{ key: string }`, the fields named in `secretFields`. Secrets arrive only in `Connect` and must never be written to disk or stderr.
- **Role**: `"browse" | "job"`. Two sessions per location at most; `job` sessions serve transfers and mirror runs so a cancelled job never disturbs browsing.

## Messages

### Description and validation

| Request | Fields | Reply |
|---|---|---|
| `Describe` | | `{ "scheme", "displayName", "version": string, "form": [Field], "defaults": Config, "secretFields": [string], "detector": { "upload": "sizeMtime" \| "sizeOnly" \| "digest", "download": … }, "features": { "setMtime": bool, "mode": bool, "realDirs": bool, "digestKind": string \| null, "separator": string, "metaInScan": bool, "pipelining": bool } }` |
| `Validate` | `config` | `{}` or `Invalid` with `field` |

`detector` tells the mirror engine which change detector to use by direction (`upload` = local master). `metaInScan: true` promises that `Scan` entries carry `meta`.

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
| `Cancel` | `id` | | `{}`; the cancelled request replies `Cancelled` |

Streams: only one `Read` or `Write` stream is active per pipe at a time; the daemon serialises them and uses the `job` session for transfers so browsing requests (`Scan`, `Stat`) stay responsive on the `browse` session. `Write` creates or truncates. A plugin should keep several protocol requests in flight for `Read` and `Write` when the protocol allows (`pipelining: true`); SFTP throughput depends on it.

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
- Keep secrets in memory only; the daemon owns the keyring.
- Log to stderr; it is captured with your scheme as the prefix.

A stub plugin (`kiki-plugin-stub`, in the test tree) implements this protocol over an in-memory tree with a two-field form and is what the daemon's contract tests run against.

## Share plugins

A second plugin kind, `kiki-plugin-share-<id>` in the same plugin directory, uses the same framing, lifecycle, error codes and `Field` type, with its own request set: `Describe`, `Configure`, `Targets`, `Share` (streamed `Progress`), `Cancel`, `Ping`, `Shutdown`. Files always arrive as local `file://` URIs; the daemon fetches remote and device files and compresses folders beforehand. The full contract is in `docs/0.1.0/18-share.md`.

## Compatibility rules

- The daemon ignores unknown fields in replies; plugins must ignore unknown fields in requests.
- New request types may be added within a version; a plugin returns `Unsupported` for any it does not know.
- Removing a field or changing its type bumps the protocol version, which the daemon checks in `Describe`'s `version` prefix (`1.x`).
