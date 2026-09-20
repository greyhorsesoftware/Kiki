# kikid socket API

The protocol between the kiki shell (or any client: the CLI, tests, another Quickshell shell) and the `kikid` daemon. This document is the authority; feature plans describe behaviour, this describes the wire.

Version: `1` (0.1.0). Clients send `Hello` first; a daemon that cannot serve the client's version replies with an error and closes.

## Transport and framing

- Unix stream socket at `$XDG_RUNTIME_DIR/kiki.sock`, one connection per client. The daemon is socket-activated by systemd and stays running.
- Two framings, chosen per connection by the first byte the client sends. A `{` selects **text framing**: newline-delimited JSON, one object per line, which is what the QML shell uses since it reads a line stream and never needs binary frames. Anything else selects **binary framing**, used by tools and tests that stream bytes:
- Every binary frame is: `u32` little-endian payload length, `u8` frame type, payload.
  - type `0x00`: JSON. The payload is one UTF-8 JSON object.
  - type `0x01`: binary. Raw bytes belonging to the most recent streaming request on this connection (see `Read`/`Write` below). A zero-length binary frame ends a stream.
- Maximum JSON frame: 16 MiB. Maximum binary frame: 1 MiB (senders chunk larger data).
- Frames from one side are processed in order. Requests may be pipelined; replies carry the request id and may interleave with events.

## Message shapes

Three kinds of JSON object:

```
request   { "id": 17, "type": "Window", ...fields }
reply     { "id": 17, "ok": { ...result } }           or   { "id": 17, "err": { "code": "NotFound", "message": "…" } }
event     { "event": "Rows", ...fields }                (no id; daemon → client, unsolicited)
```

`id` is a client-chosen integer, unique per connection while the request is outstanding. Every request gets exactly one reply, except streaming requests, which get one reply after the stream ends.

Error codes: `NotFound`, `Denied`, `Exists`, `NotEmpty`, `Unsupported`, `Cancelled`, `Busy`, `Io`, `Protocol`, `Plugin` (a plugin failed; `message` carries its text), `Safety` (a mirror guard refused). `message` is for humans; clients branch on `code`.

## Types

- **Uri**: string. `file:///home/david/Projects`, `sftp://homelab/srv/kiki`, `ftps://nas/volume1`. Percent-encoded, absolute. A bare absolute path is accepted where a Uri is expected and treated as `file://`.
- **Kind**: `"folder" | "file" | "link" | "image" | "video" | "audio" | "document" | "pdf" | "text" | "code" | "archive" | "other"`. Folders and links come from `d_type`; the rest from the extension in phase 1 and the mime type once phase 2 has run.
- **Meta**: `{ "size": u64, "mtime": u64 (ms since epoch, 0 = unknown), "atime": u64 (last access, ms, 0 = unknown; local files only, as fresh as the mount's atime policy), "mode": u32 | null, "owner": string | null, "group": string | null, "digest": { "kind": "md5", "hex": string } | null }`
- **Row**: `{ "name": string, "kind": Kind, "isDir": bool, "isLink": bool, "meta": Meta | null, "thumb": string | null, "git": Git | null }`. `thumb` is an absolute path into the thumbnail cache; `null` until generated, `""` if generation failed. `git` is `null` outside a repository or before status has run.
- **Git**: `{ "state": "modified" | "added" | "deleted" | "renamed" | "conflicted" | "untracked" | "ignored" | "clean", "staged": bool }`
- **Job**: `{ "id": u64, "op": string, "state": "queued" | "running" | "done" | "failed" | "cancelled", "done": u64, "total": u64, "bytes": u64, "bytesTotal": u64, "title": string, "error": string | null, "undoable": bool }`
- **Location**: `{ "name": string, "plugin": string, "remoteUri": Uri, "localUri": Uri, "config": { key: string } }` (never secrets)
- **Field** (form description from a plugin): `{ "key": string, "label": string, "kind": "text" | "password" | "path" | "port" | "select" | "file", "required": bool, "default": string | null, "options": [string] | null, "group": string | null }`

## Session

| Request | Fields | Reply |
|---|---|---|
| `Hello` | `version: u32`, `client: string` | `{ version, daemon: string, plugins: [string] }` |
| `Ping` | | `{}` |
| `Version` | | `{ version: string }` |

## Listings

A listing is opened with a client-chosen `lid` so that `Open` and the first `Window` can be sent in one write.

| Request | Fields | Reply |
|---|---|---|
| `Open` | `lid: u32`, `uri: Uri` | `{ cached: bool }` |
| `Window` | `lid`, `first: u32`, `count: u32` (max 512) | `{ first, rows: [Row], n: u32, done: bool }` in the current sort and filter; becomes the connection's live window for this `lid` |
| `Sort` | `lid`, `role: "name" \| "kind" \| "size" \| "mtime" \| "atime"`, `order: "asc" \| "desc"` | `{ n }`; a `Reset` event follows when the order is applied (immediately when cached, after an `Enrich` pass for size and mtime) |
| `Filter` | `lid`, `text: string` (substring, case-insensitive; empty clears) | `{ n }` then `Reset` |
| `SeekName` | `lid`, `prefix`, `after?` | `{ index }` first view position whose name starts with `prefix` (case-insensitive), after `after` with wrap; `null` when none (type-ahead, plan 23) |
| `ShowHidden` | `lid`, `show: bool` | `{ n }`; dot-files enter or leave the view, `Reset` follows (default from `settings view.showHidden`) |
| `Enrich` | `lid` | `{}` when every row has `meta`; `Progress` events meanwhile |
| `Close` | `lid` | `{}` |
| `Prefetch` | `uri` | `{}`; scans into the cache with no live window |
| `Stat` | `uri` | `Meta` |
| `Refresh` | `lid` | `{}`; drops the cache entry and rescans (`Reset` follows) |

Events for listings:

| Event | Fields | Meaning |
|---|---|---|
| `Count` | `lid`, `n`, `done` | phase 1 progress; `done: true` once, when the scan completes |
| `Rows` | `lid`, `first`, `rows: [Row]` | pushed rows inside the live window whose `meta` or `thumb` changed, or that a watch patched |
| `Reset` | `lid`, `n` | the order or membership changed; the client discards its window cache and re-requests |
| `Progress` | `lid`, `done`, `total` | `Enrich` progress |
| `Gone` | `lid` | the directory was deleted or the location disconnected |

Semantics: `Window` is answered immediately with whatever is known; missing `meta` for those rows is fetched nearest-centre-first and pushed as `Rows`. Rows outside the live window are never stated. Thumbnails for rows in the live window are generated at low priority and pushed the same way. The daemon keeps one live window per `lid` per connection.

## Search

| Request | Fields | Reply |
|---|---|---|
| `Search` | `lid`, `scope: "everywhere" \| "location"`, `uri` (for `location`), `query`, `mode: "substring" \| "prefix" \| "fuzzy"` | `{ n, capped: bool, indexAge: u64 }`; results are a listing under `lid` (rows carry `parent: Uri`), followed by `Count`/`Reset` |
| `IndexStatus` | | `{ entries, dirs, roots: [Uri], builtAt: u64, refreshing: bool }` |
| `IndexRebuild` | | `{}` |
| `IndexRoots` | | `{ roots: [Uri] }` |
| `SetIndexRoots` | `roots` | `{}` |

Event: `IndexProgress { done, total }`. In-folder search is `Filter` on the open listing.

Git event: `RepoChanged { root }`.

## Previews and thumbnails

| Request | Fields | Reply |
|---|---|---|
| `Preview` | `uri` | one of `{ text: string, bytesRead: u64, truncated: bool }`, `{ path: string, width, height }` (image, video frame, PDF page rendered into the cache), `{ children: [string], n }`, `{ members: [{ name, size, isDir }], n }` |
| `Thumbnail` | `uri`, `size: 128 \| 256` | `{ path: string }` or error `Unsupported` |
| `GitStatus` | `uri` | `{ state, staged, branch, last: { hash, short, author, time, subject } \| null }` |
| `Repo` | `uri` | `{ root: Uri, branch: string \| null, detached: bool, ahead: u32, behind: u32, dirty: bool } \| null` |
| `GitRefresh` | `uri` | `{}` |

## Code viewer and editor (plan 13)

| Request | Fields | Reply |
|---|---|---|

Editor launch and sessions are the Open in messages above with the entry marked `role = "editor"`.

## Project mode and tree (plan 16)

| Request | Fields | Reply |
|---|---|---|
| `OpenTree` | `lid`, `uri` | `{ n }`; rows via `Window` as `Row` + `{ depth, expanded, rel }` |
| `TreeExpand` | `lid`, `first`, `expanded: bool` | `{ n }` then `Reset` |
| `TreeFilter` | `lid`, `text` | `{ n }` then `Reset` |
| `TreeReveal` | `lid`, `uri` | `{ row }` |
| `Arrange` | `layout: "project"`, `root: Uri`, `windows: [{ role, class, pid }]` | `{ arranged: [string], missing: [string] }` |

## Open in… (plan 14)

| Request | Fields | Reply |
|---|---|---|
| `OpenInList` | | `{ tools: [{ id, name, icon, accepts, enabled, reason? }] }` |
| `OpenIn` | `id` or `role`, `uris: [Uri]`, `line?` | `{ pid, reused: bool }` |
| `OpenInTest` | `id`, `uris` | `{ command: string, cwd: string }` |
| `OpenInSessions` | | `{ sessions: [{ id, pid, files: [Uri] }] }` |
| `OpenInClose` | `id` | `{}` |

Event: `OpenInChanged {}`.

## Jobs

| Request | Fields | Reply |
|---|---|---|
| `Submit` | `op: Op` | `{ job: u64 }` — ops include `restore { names }` (from the trash, undoable) and `emptyTrash {}` (not undoable) |
| `Cancel` | `job` | `{}` |
| `Jobs` | | `{ jobs: [Job] }` (running and the last 50 finished) |
| `Undo` | | `{ job }` or error `NotFound` if the journal is empty |
| `Redo` | | `{ job }` |
| `PromptReply` | `job`, `choice: string`, `applyToAll: bool` | `{}` |

`Op` is an object with `"op"` and fields:

| op | fields | undoable |
|---|---|---|
| `copy` | `items: [Uri]`, `dest: Uri` | yes |
| `move` | `items`, `dest` | yes |
| `rename` | `uri`, `name: string` | yes |
| `trash` | `items` | yes |
| `delete` | `items` | no (remote only; confirmed by the client first) |
| `mkdir` | `uri` | yes |
| `chmod` | `items`, `mode: u32`, `recursive: bool` | yes |
| `compress` | `items`, `archive: Uri`, `format: "zip" \| "tar" \| "tar.gz" \| "tar.xz" \| "tar.zst"` | yes |
| `extract` | `archive: Uri`, `dest: Uri` | yes |
| `mirrorScan` | `spec: MirrorSpec` | no (read-only; result is a plan) |
| `mirrorRun` | `spec`, `plan: u64` (a plan listing id from `MirrorPlan`) | no |

Events:

| Event | Fields | Meaning |
|---|---|---|
| `JobEvent` | `job: Job` | state change, or progress at most every 100 ms |
| `Prompt` | `job`, `kind: "collision"`, `uri`, `existing: Meta`, `incoming: Meta`, `choices: ["replace","keepBoth","skip"]` | the job is paused until `PromptReply` |
| `Toast` | `job`, `text`, `undoable` | a completed destructive job the shell should announce |

## Favorites and locations

| Request | Fields | Reply |
|---|---|---|
| `Favorites` | | `{ items: [{ name, uri }] }` |
| `SetFavorites` | `items` | `{}` |
| `ViewPrefs` | | `{ folders: { <uri>: { view, sort, order, hidden? } } }` per-folder view memory |
| `SetViewPref` | `uri`, `view`, `sort`, `order`, `hidden?` | `{}`; emits `ViewPrefsChanged { uri }` |
| `ClearViewPrefs` | | `{}` |
| `Integration` | | `{ mime, dbus, hypr, portal: bool, hyprlandAvailable, hyprConfigErrors: [string], mimeapps, bindings, portals, services: path }` (plan 09) |
| `Integrate` | `parts?: ["mime" \| "dbus" \| "hypr" \| "portal"]` | `{ results: [{ part, ok, message }], status }`; all parts when omitted |
| `Unintegrate` | `parts?` | same shape; removes exactly kiki's entries |
| `Volumes` | | `{ items: [{ name, uri, device, fsType, free: u64, total: u64, removable: bool, mounted: bool, atimeSupport: "noatime" \| "relatime" \| "strictatime" \| "unknown", size? }] }` — unmounted filesystems (from `lsblk`) have `mounted: false`, an empty `uri` and their `size` string |
| `Mount` | `device` | `{ uri, mountPoint }` via `udisksctl mount`; emits `VolumesChanged` |
| `Unmount` | `device` | `{}` |
| `Eject` | `device` or `uri` | `{}` — a block device is unmounted and powered off; a device URI (plan 17) closes the plugin session and hides the device until re-plug |
| `AccessLog` | `uris` | `{ opened: { <uri>: ms }, entries, path }` kiki's own open times (plan 22); rows also carry `opened` when known |
| `ClearAccessLog` | | `{}` |
| `TrashInfo` | | `{ items: [{ name, path, deleted }] }` from `info/*.trashinfo` (plan 04 Trash view) |
| `OpenWith` | `uri` | `{ mime, apps: [{ id, name, icon, default: bool }] }` desktop entries for the file's MIME type |
| `Launch` | `app`, `uris` | `{}` runs the desktop entry with its `Exec` expanded |
| `Plugins` | | `{ plugins: [{ scheme, displayName, form: [Field], defaults: {…}, secretFields: [string] }] }` |
| `Locations` | | `{ locations: [Location] }` |
| `TestLocation` | `location: Location`, `secrets: { key: string }` | `{}` or error with `field` |
| `AddLocation` | `location`, `secrets`, `trust`? | `{}` when saved (validates, connects, writes keyring then file), or `{ verify, host }` when the server offered a key nobody has accepted — nothing is saved, and the shell asks again with `trust` set to the fingerprint it showed, which is stored as `config.trustedFingerprint` |
| `UpdateLocation` | `location`, `secrets` (only changed keys), `trust`? | as `AddLocation` |
| `Icon` | `name`, `theme`, `size` | `{ path }` — the file an icon theme uses for a freedesktop icon name, or `null`. Resolved by the daemon because Qt answers with a provider URL that does not change when the theme does |
| `PluginBrowse` | `plugin`, `field`, `config`, `secrets` | `{ items: [{ value, label }] }` — the choices a plugin offers for a `browse` field (shares on a server, say) |
| `RemoveLocation` | `name` | `{}` |
| `Disconnect` | `name` | `{}` |
| `Settings` | | the merged `settings.toml` as an object with the daemon's defaults filled in |
| `SetSettings` | `patch: object` | `{}` (deep-merged into the file) |

Events: `LocationsChanged {}`, `VolumesChanged {}`, `FavoritesChanged {}`.

## Share (plan 18)

| Request | Fields | Reply |
|---|---|---|
| `SharePlugins` | | `{ plugins: [{ id, name, icon, accepts, targets, form, compose, enabled, configured }] }` |
| `ShareTargets` | `plugin`, `query?` | `{ targets: [{ id, name, detail, online, icon }] }` |
| `Share` | `plugin`, `uris`, `target?`, `compose` | `{ job }` |
| `ShareConfigure` | `plugin`, `config`, `secrets` | `{}` |

Event: `SharePluginsChanged {}`.

## Devices (plan 17)

| Request | Fields | Reply |
|---|---|---|
| `Devices` | | `{ devices: [{ uri, kind: "ptp" \| "mtp" \| "afc", name, vendor, model, serial, connected: bool, busy: string \| null }] }` |
| `Eject` | `uri` | `{}` (shared with the volume form above) |
| `RenameDevice` | `uri`, `name` | `{}` writes the display name to `devices.toml` |

Events: `DeviceAdded { device }`, `DeviceRemoved { uri }`. A device URI resolves like a location: the daemon hands the plugin a transient location whose `config` carries `serial`, `vendor`, `model`, `bus`, `devnum` and the sysfs path.

## Mirror

`MirrorSpec`: `{ "master": Uri, "replica": Uri, "direction": "upload" | "download", "deleteExtras": bool, "blastRadius": f64, "confirmedLargeDelete": bool, "clockOffsetMs": i64, "clockOffsetAuto": bool, "detector": "auto" | "sizeMtime" | "sizeOnly" | "digest", "modifiedWithinMs": u64 | null, "applyFilters": bool }`

| Request | Fields | Reply |
|---|---|---|
| `MirrorPlan` | `job` (a finished `mirrorScan`), `lid` | opens the plan as a listing under `lid`: `{ counts: { new, changed, equal, extra, deletes, copyBytes, replicaEntries, filtered }, clockOffsetMs }` |
| `MirrorFilter` | `lid`, `reason: "all" \| "new" \| "changed" \| "equal" \| "delete"` | `{ n }` then `Reset` |
| `MirrorCheck` | `lid`, `first`, `count`, `checked: bool` | `{}` |
| `MirrorReport` | `job` | `{ text: string }` |
| `Filters` / `SetFilters` | — / `rules: [{ kind: "contains" \| "startsWith" \| "endsWith" \| "matches", value }]` | `{ rules }` / `{}` |

Plan rows come through `Window` on the plan `lid` as `{ "rel": string, "action": "copy" | "mkdir" | "delete" | "rmdir" | "skip", "reason": "new" | "changed" | "extra" | "equal", "bytes": u64, "checked": bool, "master": Meta | null, "replica": Meta | null, "state": "pending" | "running" | "done" | "skipped" | null, "progress": u8 | null }`. During `mirrorRun` the same `lid` receives `Rows` events as actions change state.

## Session, settings and keymap additions

| Request | Fields | Reply |
|---|---|---|
| `Keymap` | | `{ keys: [{ key, action, plan }] }` |
| `PluginStatus` | | `{ plugins: [{ name, kind, path, running, describe }] }` |
| `PluginPing` | `name` | `{ ms, describe }` (spawns, describes, pings, exits) |
| `About` | | `{ version, socket, pluginDir, helperDir, configDir }` |
| `ResetSettings` | | `{}` |
| `ChooserResult` | `token`, `uris: [Uri] \| null` | `{}` (the shell's answer to `ShowChooser`) |
| `SetOpenIn` | `tools: [tool]` | `{}` |
| `AiStatus` | | `{ configured, provider, chosenBy: "omarchy" \| "settings" \| "default", omarchyProvider, cli, cliAvailable, providers }` |
| `AiConfigure` | `provider?` (`omarchy` to follow Omarchy), `cliCommand?` | `AiStatus` result |
| `AiOpen` | `dir`, `uris: [Uri]` | `{ tool }` — starts the chosen AI's command-line tool for a conversation, in a terminal window opened in `dir`, told which files are selected. Local only |
| `OpenTerminal` | `dir` | `{}` — a terminal window in `dir` |

Tree views (plan 16): `OpenTree`, `TreeExpand`, `TreeFilter`, `TreeReveal`, `Arrange` as listed under Project mode; the tree's rows come through `Window`.

Share (plan 18): `Share` submits a `share` job; progress arrives on `JobEvents`.

## Daemon-initiated requests

The daemon needs the shell for two things. These are events that expect an answering request.

| Event | Fields | Client answers with |
|---|---|---|
| `ShowChooser` | `token: string`, `mode: "open" \| "save" \| "saveFiles"`, `title`, `multiple: bool`, `directory: bool`, `filters: [{ name, patterns: [string] }]`, `currentFolder: Uri \| null`, `currentName: string \| null`, `parentWindow: string \| null` | `ChooserResult { token, uris: [Uri] \| null, filter: u32 \| null }` (`null` uris = cancelled) |
| `ShowItems` | `uris: [Uri]`, `properties: bool` | nothing; the shell opens or focuses a window, selects the items, and opens the inspector when `properties` |

## Examples

Open a directory and ask for the first screen in one write:

```
→ {"id":1,"type":"Open","lid":7,"uri":"file:///home/david"}
→ {"id":2,"type":"Window","lid":7,"first":0,"count":60}
← {"id":1,"ok":{"cached":false}}
← {"event":"Count","lid":7,"n":1024,"done":false}
← {"id":2,"ok":{"first":0,"n":1024,"done":false,"rows":[{"name":"Desktop","kind":"folder","isDir":true,"isLink":false,"meta":null,"thumb":null}, …]}}
← {"event":"Count","lid":7,"n":10312,"done":true}
← {"event":"Rows","lid":7,"first":0,"rows":[{"name":"Desktop","kind":"folder","isDir":true,"isLink":false,"meta":{"size":4096,"mtime":1789224840000,"mode":16877,"owner":"david","group":"david","digest":null},"thumb":null}, …]}
```

Trash a file and undo it:

```
→ {"id":3,"type":"Submit","op":{"op":"trash","items":["file:///home/david/wallpapers.zip"]}}
← {"id":3,"ok":{"job":41}}
← {"event":"JobEvent","job":{"id":41,"op":"trash","state":"done","done":1,"total":1,"bytes":0,"bytesTotal":0,"title":"Move wallpapers.zip to Trash","error":null,"undoable":true}}
← {"event":"Toast","job":41,"text":"Moved wallpapers.zip to Trash","undoable":true}
→ {"id":4,"type":"Undo"}
← {"id":4,"ok":{"job":42}}
```

## Compatibility rules

- New fields may be added to any object; clients ignore unknown fields.
- New request types, events and error codes may be added within a version.
- Removing or renaming a field, or changing a type, bumps `version`.
