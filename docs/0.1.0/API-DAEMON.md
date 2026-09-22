# kikid socket API

The protocol between the kiki shell (or any client: the CLI, tests, another Quickshell shell) and the `kikid` daemon. This document is the authority; feature plans describe behaviour, this describes the wire.

Version: `1` (0.1.0). Clients send `Hello` first; a daemon that cannot serve the client's version replies with an error and closes.

## Transport and framing

- Unix stream socket at `$XDG_RUNTIME_DIR/kiki.sock`, one connection per client. The daemon is socket-activated by systemd and stays running.
- Two framings, chosen per connection by the first byte the client sends. A `{` selects **text framing**: newline-delimited JSON, one object per line, which is what the QML shell uses since it reads a line stream. Anything else selects **length-prefixed framing**, used by tools and tests:
- Every framed message is: `u32` little-endian payload length, `u8` frame type, payload.
  - type `0x00`: JSON. The payload is one UTF-8 JSON object.
  - type `0x01`: binary. **Not used on this socket**: no request streams bytes, and a binary frame that arrives is answered `Protocol`. The type is here because the framing is shared with the plugin pipe (`API-PLUGIN.md`), where it carries file bytes. File contents never cross this socket — jobs move files, and previews and thumbnails travel as paths into on-disk caches.
- Maximum JSON frame: 16 MiB.
- Frames from one side are processed in order. Requests may be pipelined; replies carry the request id and may interleave with events.

## Message shapes

Three kinds of JSON object:

```
request   { "id": 17, "type": "Window", ...fields }
reply     { "id": 17, "ok": { ...result } }           or   { "id": 17, "err": { "code": "NotFound", "message": "…" } }
event     { "event": "Rows", ...fields }                (no id; daemon → client, unsolicited)
```

`id` is a client-chosen integer, unique per connection while the request is outstanding. Every request gets exactly one reply. A request that starts a job replies with the job's id and goes on in `JobEvent`s.

Error codes: `NotFound`, `Denied`, `Exists`, `NotEmpty`, `Unsupported`, `Unsafe` (a guard refused — an archive member reaching outside its destination), `Io`, `Protocol` (the request did not parse, or a field is missing), `Invalid` (a value was refused; `field` names it where one does), `Version` (`Hello` named a protocol this daemon does not speak). `message` is for humans; clients branch on `code`. A plugin's refusal is mapped onto these — `Auth` becomes `Denied`, anything else it has no equivalent for becomes `Io` with the plugin's own code and text in `message` — so a client never sees a code from the plugin protocol.

## Types

- **Uri**: string. `file:///home/david/Projects`, `sftp://homelab/srv/kiki`, `ftps://nas/volume1`. Percent-encoded, absolute. A bare absolute path is accepted where a Uri is expected and treated as `file://`.
- **Kind**: `"folder" | "file" | "link" | "image" | "video" | "audio" | "document" | "pdf" | "text" | "code" | "archive" | "other"`. Folders and links come from `d_type`; the rest from the extension in phase 1 and the mime type once phase 2 has run.
- **Meta**: `{ "size": u64, "mtime": u64 (ms since epoch, 0 = unknown), "atime": u64 (last access, ms, 0 = unknown; local files only, as fresh as the mount's atime policy), "mode": u32 | null, "owner": string | null, "group": string | null, "digest": { "kind": "md5", "hex": string } | null }`
- **Row**: `{ "name": string, "kind": Kind, "isDir": bool, "isLink": bool, "meta": Meta | null, "thumb": string | null, "git": Git | null }`. `thumb` is an absolute path into the thumbnail cache; `null` until generated, `""` if generation failed. `git` is `null` outside a repository or before status has run.
- **Git**: `{ "state": "modified" | "added" | "deleted" | "renamed" | "conflicted" | "untracked" | "ignored" | "clean", "staged": bool }` On a folder that is itself a repository it also carries `{ "root": true, "branch": string, "detached": bool }` — the branch (or, detached, the short hash), with `state` the aggregate of that repository rather than of the folder above it; the branch arrives with the listing and the `state` a moment later as a `Rows` update. `root` is absent on every other row.
- **Job**: `{ "id": u64, "op": string, "state": "queued" | "running" | "done" | "failed" | "cancelled", "done": u64, "total": u64, "bytes": u64, "bytesTotal": u64, "title": string, "error": string | null, "undoable": bool }` and, for the activity view (plan 32), on every job:

| Field | Meaning |
|---|---|
| `name`, `count`, `isDir` | the entry's icon and header: the file or folder the job is about (the first of several) and how many there are, rather than the prose `title`. `isDir` for something on a server is not knowable when the job is submitted; the transfer says once it has listed it |
| `src`, `dest` | absent when the op has none |
| `direction` | `"upload" \| "download" \| "remote"` (server to server) `\| "local"` |
| `phase` | `"preparing" \| "running"` — **derived**: a running job with no totals and no bytes yet is preparing (a transfer walks the whole tree before `set_totals`, which broadcasts as soon as it knows) |
| `cancelling` | derived, and broadcast by `Cancel` itself, so the header flips at once instead of when the worker notices |
| `current` | `{ name, bytes, size } \| null` — the file in hand, reported only while the job runs. `size` is 0 where the bytes moving belong to several workers (a mirror) |
| `rate` | bytes/s, smoothed, from the daemon so every window shows the same number; 0 unless running |
| `result` | `{ copies, deletes, skipped } \| null` — a mirror run's completion line |
| `revealUri` | the first local thing the job created, for Reveal; absent otherwise |
| `hidden` | machinery rather than something the user asked for (`movePairs`, `rmdirIfEmpty`, `chmodList`, `mirrorScan`, silent undo deletes). Kept out of the activity popup |

  `error` names the failing path (`site/img/c.bin: Denied`), and a job that stopped part way says so.
- **Location**: `{ "name": string, "plugin": string, "remoteUri": Uri, "localUri": Uri, "config": { key: string } }` (never secrets)
- **Field** (form description from a plugin): `{ "key": string, "label": string, "kind": "text" | "password" | "path" | "port" | "select" | "file", "required": bool, "default": string | null, "options": [string] | null, "group": string | null }`

## Session

| Request | Fields | Reply |
|---|---|---|
| `Hello` | `version: u32`, `client: string` | `{ version, daemon: string, plugins: [string] }` |
| `Ping` | | `{}` |
| `Version` | | `{ version: string }` |

`client: "kiki"` says the connection is a kiki window, and that has a consequence: the jobs a window starts (`Submit`, `Share`, `Undo`, `Redo`) are stopped when the last window has disconnected and none has come back within a short grace (4 s; `KIKI_SHELL_GRACE_MS`). Nothing runs on behind a kiki that is gone. Any other client name — a script, the portal, a test — owns its jobs outright and they are never cancelled for it.

## Listings

A listing is opened with a client-chosen `lid` so that `Open` and the first `Window` can be sent in one write.

| Request | Fields | Reply |
|---|---|---|
| `Open` | `lid: u32`, `uri: Uri` | `{ cached: bool }` |
| `Window` | `lid`, `first: u32`, `count: u32` (max 512), `viewFirst?`, `viewCount?` | `{ first, rows: [Row], n: u32, done: bool, gen: u64 }` in the current sort and filter; becomes the connection's live window for this `lid`. `gen` numbers the state of the view the rows describe (below). `viewFirst`/`viewCount` say which of those rows are **on screen** rather than held against a scroll; thumbnails are made for those only. Omit them and the whole range counts as on screen |
| `Sort` | `lid`, `role: "name" \| "kind" \| "size" \| "mtime" \| "atime"`, `order: "asc" \| "desc"` | `{ n }`; a `Reset` event follows when the order is applied (immediately when cached, after an `Enrich` pass for size and mtime) |
| `Filter` | `lid`, `text: string` (substring, case-insensitive; empty clears) | `{ n }` then `Reset` |
| `SeekName` | `lid`, `prefix`, `after?` | `{ index }` first view position whose name starts with `prefix` (case-insensitive), after `after` with wrap; `null` when none (type-ahead, plan 23) |
| `ShowHidden` | `lid`, `show: bool` | `{ n }`; dot-files enter or leave the view, `Reset` follows (default from `settings view.showHidden`) |
| `Enrich` | `lid` | `{}` when every row has `meta`; `Progress` events meanwhile |
| `Close` | `lid` | `{}` |
| `Stat` | `uri` | `Meta` |
| `Refresh` | `lid` | `{}`; drops the cache entry and rescans (`Reset` follows) |

Events for listings:

| Event | Fields | Meaning |
|---|---|---|
| `Count` | `lid`, `n`, `done` | phase 1 progress; `done: true` once, when the scan completes |
| `Rows` | `lid`, `first`, `rows: [Row]` | pushed rows inside the live window whose `meta` or `thumb` changed, or that a watch patched |
| `Reset` | `lid`, `n`, `gen` | the order or membership changed; the client re-requests its window, and may go on showing the rows it holds until the answer arrives (a rescan brings the same rows back) |
| `Splice` | `lid`, `n`, `gen`, `ops: [{ op: "remove", pos } \| { op: "insert", pos, row: Row }]` | a few rows came or went in place (a watched folder, name or kind order, no filter; at most 64 changes — anything else is a `Reset`). Applied in order, each `pos` as of that step; every held position after it moves by one. `n` is the count afterwards |
| `Progress` | `lid`, `done`, `total` | `Enrich` progress |
| `Gone` | `lid` | the directory was deleted or the location disconnected |

`gen` counts changes to the view (sort, filter, rescan, splice). A `Window` reply and the event that announces a change travel separately and can arrive in either order, so the client compares numbers: a reply with a `gen` lower than the last `Reset` or `Splice` was computed before it — its positions are the old ones — and is asked again; a `Splice` whose `gen` the held rows already have is not applied a second time; a `Splice` more than one ahead means a step was missed, and is treated as a `Reset`. Trees and search results do not number their answers, and a client takes those as they come.

A rescan keeps, by name, what the listing knew: metadata, thumbnail (unless the scan reports another modification time) and git state (shown as it was until status has run again).

Semantics: `Window` is answered immediately with whatever is known; missing `meta` for those rows is fetched nearest-centre-first and pushed as `Rows`. Rows outside the live window are never stated. Thumbnails for rows in the live window are generated at low priority and pushed the same way. The daemon keeps one live window per `lid` per connection.

## Search

| Request | Fields | Reply |
|---|---|---|
| `Search` | `lid`, `scope: "everywhere" \| "location"`, `uri` (for `location`), `query`, `mode: "substring" \| "prefix" \| "fuzzy"` | `{ n, capped: bool, indexAge: u64 }`; results are a listing under `lid` (rows carry `parent: Uri`), followed by `Count`/`Reset` |
| `IndexStatus` | | `{ entries, dirs, roots: [Uri], builtAt: u64, refreshing: bool }` |
| `IndexRebuild` | | `{}` |
| `IndexRoots` | | `{ roots: [Uri] }` |
| `SetIndexRoots` | `roots` | `{}` |

In-folder search is `Filter` on the open listing. There is no index-progress event: `IndexStatus`'s `refreshing` is what a client watches.

Git event: `RepoChanged { root }`.

## Previews and thumbnails

| Request | Fields | Reply |
|---|---|---|
| `Preview` | `uri` | one of `{ text: string, bytesRead: u64, truncated: bool }`, `{ path: string, width, height }` (image, video frame, PDF page rendered into the cache), `{ children: [string], n }`, `{ members: [{ name, size, isDir }], n }` |
| `Thumbnail` | `uri`, `size: 128 \| 256` | `{ path: string }` or error `Unsupported` |
| `GitStatus` | `uri` | `{ state, staged, branch, last: { hash, short, author, time, subject } \| null }` |
| `Repo` | `uri` | `{ root: Uri, branch: string \| null, detached: bool, ahead: u32, behind: u32, dirty: bool } \| null` |
| `GitRefresh` | `uri` | `{}` |

Thumbnails are made by **`kiki-thumber`, a process of its own** — the only part of kiki that decodes
a file's contents, so that a malformed picture costs that file its thumbnail rather than the daemon.
It speaks the plugin framing of `API-PLUGIN.md` (run through `Plugin::spawn_path`) without being a
location plugin: no `Describe`, no `Connect`, no location. `{"type":"Thumb", uri, kind, size, mtime}`
→ `{"ok":{"path":…}}`, and "no thumbnail for this one" is that same `ok` with a **null path** rather
than an `err`, so the daemon can tell it from the child dying. The pixels never cross the pipe: the
child writes the cache file and answers with its path. The daemon serves a cache hit itself, has one
request out per worker, kills a child stuck 20 s on one file, and gives up on a file that kills one
twice.
`Thumbnail` and `Preview` therefore answer a little later on a cold cache and `Unsupported` when
there is no thumbnail to be had — callers see no other difference. Installed at
`/usr/lib/kiki/kiki-thumber`; without it the daemon says so once and makes no thumbnails.

Thumbnails are made for `file://` only (owner, 2026-09-20), so `Thumbnail` and a picture's `Preview`
answer `Unsupported` for a remote URI and a remote row's `thumb` is `null` — the views draw the kind
artwork for it. A thumbnail already in the cache under a remote URI is still served. `Uri::to_path`
drops the scheme and the host, so asking otherwise reads *this* machine's copy of that path. The
plugin SDK has a `thumb` hook for when remote thumbnails are built (post-0.1.0); only PTP implements
it today, and the daemon does not yet send `Thumb`.

## Editor bridge (plan 13)

No requests of its own. kiki has no built-in code viewer and will not grow one, so there is no `OpenText`, no `TextFind` and no highlighting service; editor launch and sessions are the Open in messages below, with the entry marked `role = "editor"`.

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
| `OpenIn` | `tool` (or `role`), `uris: [Uri]`, `line?` | `{ pid, reused: bool, class: string }` — `class` is the window class the tool's terminal is given, for Arrange |
| `OpenInTest` | `tool` (or `role`), `uris`, `line?` | `{ command: string, cwd: string }` |
| `OpenInSessions` | | `{ sessions: [{ id, pid, files: [Uri] }] }` |
| `OpenInClose` | `tool` | `{}` |

**The tool is named in `tool`, never in `id`.** Every request carries the client's own numeric `id`, so a tool named in a field of that name replaced it and the request never parsed — the daemon answered "missing id" to nobody, and `Alt+Enter` had never once worked.

Event: `OpenInChanged {}`.

## Jobs

| Request | Fields | Reply |
|---|---|---|
| `Submit` | `op: Op` | `{ job: u64 }` |
| `Cancel` | `job` | `{}`; broadcasts at once, so `cancelling` is true before the worker notices |
| `Jobs` | | `{ jobs: [Job] }` — every live job plus the fifty most recent finished ones. A live job is never dropped to make room |
| `JobEvents` | | `{}`; subscribes this connection to `JobEvent`, `JobsCleared`, `Prompt` and `Toast`. A connection that has not asked is sent none of them |
| `ClearJobs` | | `{ cleared }` — forgets every finished job; live ones never (plan 32) |
| `DismissJob` | `job` | `{ cleared }` |
| `JobLog` | `job`, `from?` | `{ lines: [{ t, level, source, text }], next, dropped }` — the job's log; ask again from `next` |
| `LocationLog` | `location`, `from?` | the same, for everything the location's plugin has said |
| `Undo` | | `{ job }` or error `NotFound` if the journal is empty |
| `Redo` | | `{ job }` |
| `PromptReply` | `job`, `choice: string`, `applyToAll: bool` | `{}` |

`Op` is an object with `"op"` and fields:

| op | fields | undoable |
|---|---|---|
| `copy` | `items: [Uri]`, `dest: Uri` | yes |
| `move` | `items`, `dest` | yes — locally. A move that touches a server is not, by decision |
| `rename` | `uri`, `name: string` | yes |
| `trash` | `items` | yes (local only: a server has no trash, and the daemon refuses one) |
| `restore` | `names` | yes — back out of the trash |
| `emptyTrash` | | no |
| `delete` | `items` | no (confirmed by the client first) |
| `mkdir` | `uri` | yes |
| `chmod` | `items`, `mode: u32`, `recursive: bool` | yes |
| `compress` | `items`, `archive: Uri`, `format: "zip" \| "tar" \| "tar.gz" \| "tar.xz" \| "tar.zst" \| "tar.bz2" \| "7z"` | yes |
| `extract` | `archive: Uri`, `dest: Uri` | yes — extraction never merges: it unpacks in a staging folder and takes one free name, so the inverse is "delete that one thing" |
| `share` | `plugin`, `uris`, `target?`, `compose` | no (submitted by `Share`) |
| `mirrorScan` | `spec: MirrorSpec` | no (read-only; result is a plan) |
| `mirrorRun` | `spec`, `plan: u64` (a plan listing id from `MirrorPlan`), `workers?` (default 5, clamped 1–8) | no |

`deleteCopies` is the inverse a copy **to a server** journals: `{ dest, guard, files, dirs }`, the files and folders that copy created with the size and time the server reported as each landed. Undoing it deletes only what the server still says is that — by the detector that server's plugin asks for — leaves what has changed or gone and names it, and removes a folder only if it is empty afterwards. A copy that was cancelled or failed half way journals the part that arrived. It is not submitted by a client; `Undo` runs it. `movePairs`, `rmdirIfEmpty` and `chmodList` are inverses of the same kind, and all of them carry `hidden`.

Events:

| Event | Fields | Meaning |
|---|---|---|
| `JobEvent` | `job: Job` | state change, or progress at most every 100 ms |
| `JobsCleared` | `jobs: [id]` | finished jobs were forgotten (`ClearJobs`, `DismissJob`, or the trim) |
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
| `OpenWith` | `uris: [string]` (or `uri`, one file) | `{ mime, mimes: [string], apps: [{ id, name, icon, default: bool }] }` — the desktop entries that open **every** file asked about, in the order the first file's kind has them; `default` only when it is the default for them all; `mime` is `""` when the kinds differ |
| `Launch` | `app`, `uris` | `{}` runs the desktop entry with its `Exec` expanded: once with all the files when it takes a list (`%F`, `%U`), once for each when it takes one (`%f`, `%u`) |
| `Plugins` | | `{ plugins: [{ scheme, displayName, form: [Field], defaults: {…}, secretFields: [string] }] }` |
| `Locations` | | `{ locations: [Location] }`, each with `connected: bool` — a browse session to it is alive; `LocationsChanged` is sent when one comes or goes |
| `TestLocation` | `location: Location`, `secrets: { key: string }` | `{}` or error with `field` |
| `AddLocation` | `location`, `secrets`, `trust`? | `{}` when saved (validates, connects, writes keyring then file), or `{ verify, host }` when the server offered a key nobody has accepted — nothing is saved, and the shell asks again with `trust` set to the fingerprint it showed, which is stored as `config.trustedFingerprint` |
| `UpdateLocation` | `location`, `secrets` (only changed keys), `trust`? | as `AddLocation` |
| `Icon` | `name`, `theme`, `size` | `{ path }` — the file an icon theme uses for a freedesktop icon name, or `null`. Resolved by the daemon because Qt answers with a provider URL that does not change when the theme does |
| `PluginBrowse` | `plugin`, `field`, `config`, `secrets` | `{ items: [{ value, label }] }` — the choices a plugin offers for a `browse` field (shares on a server, say) |
| `SetLocationImage` | `name`, `image?` | `{}` — the picture the sidebar draws for a location; omit `image` to clear it. Emits `LocationsChanged` |
| `RemoveLocation` | `name` | `{}` — clears the keyring entries first (their names come from the config that is about to go), then disconnects every session the location has, job roles included, and tells the plugin each time |
| `Disconnect` | `name` | `{}` — the sidebar's and the toolbar's Disconnect. Sessions are taken out of the map first and told second: holding the map across the plugin round trip deadlocked the daemon against the job log |
| `Settings` | | the merged `settings.toml` as an object with the daemon's defaults filled in |
| `SetSettings` | `patch: object` | `{}` (deep-merged into the file) |

Events: `LocationsChanged {}`, `VolumesChanged {}`, `FavoritesChanged {}`.

## Share (plan 18)

| Request | Fields | Reply |
|---|---|---|
| `SharePlugins` | | `{ plugins: [{ id, name, icon, accepts, targets, form, compose, requires, enabled, configured, config, unavailable? }] }` — `requires` is the programs the plugin cannot work without, looked for on every listing rather than when the plugin described itself, so installing one brings the entry to life without a restart; `unavailable` is the sentence to show when something is missing. A plugin that is installed is on until the user switches it off |
| `ShareTargets` | `plugin`, `query?` | `{ targets: [{ id, name, detail, online, icon }] }` |
| `Share` | `plugin`, `uris`, `target?`, `compose` | `{ job }` — a `share` job; remote files and device files are fetched, and folders compressed, before the plugin sees them |
| `ShareConfigure` | `plugin`, `config`, `secrets` | `{}` |

No event: the plugin list is asked for when it is wanted. Mail and Tailscale ship; LocalSend was removed on 2026-09-21.

## Devices (plan 17)

**Not in 0.1.0.** No device kind is in `plugin::LOCATION_KINDS`, so detection does not run and `Devices` answers an empty list; the requests are here because the protocol keeps them for the release that ships one.

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
| `MirrorReport` | `job`, `saveTo?: Uri` | `{ text: string }`; with `saveTo`, the text is written to that local file too |
| `MirrorFilters` | | `{ rules: [Rule], defaults: bool, defaultRules: [Rule] }` — what a scan will skip. `defaults: true` means no file has been written and the built-in rules are in force; `defaultRules` travels with every answer so the dialog's "Restore defaults" shows what the daemon has rather than a copy that can drift |
| `SetMirrorFilters` | `rules: [Rule]`, or `defaults: true` | the same shape. Every rule is checked before any of it is written — the kind must be one kiki knows, the value must say something and must not carry a `/`, since a rule matches one **name** anywhere in the tree, not a path — so a refusal (`Invalid`, naming the rule) leaves the file exactly as it was. `defaults: true` deletes the file; an empty `rules` is stored as `defaults = false`, because an empty file means the defaults |

`Rule`: `{ "kind": "contains" | "startsWith" | "endsWith" | "matches", "value": string }`, where `matches` is the whole name and is what the dialog calls "is". The nine defaults are all `matches`: `.git`, `.gitignore`, `.DS_Store`, `.env`, `.idea`, `.vscode`, `Thumbs.db`, `node_modules`, `__pycache__`. A dotted name is mirrored like any other unless a rule names it; whoever wants every hidden name skipped adds `startsWith "."`.

Plan rows come through `Window` on the plan `lid` as `{ "rel": string, "action": "copy" | "mkdir" | "delete" | "rmdir" | "skip", "reason": "new" | "changed" | "extra" | "equal", "bytes": u64, "checked": bool, "master": Meta | null, "replica": Meta | null, "state": "pending" | "running" | "done" | "skipped" | null, "progress": u8 | null }`. During `mirrorRun` the same `lid` receives `Rows` events as actions change state.

## Session and settings additions

There is no `Keymap` request: the keymap is `qml/kiki/Keymap.qml`, which the rebinding window edits and `Shell.qml` reads, with the changed chords in `settings.toml` under `[keys]`. The daemon's copy of the table was read by nothing and is gone.

| Request | Fields | Reply |
|---|---|---|
| `PluginStatus` | | `{ plugins: [{ name, kind, path, running, describe }] }` — every `kiki-plugin-*` binary in the plugin directories, whether or not this build ships its kind |
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

Share (plan 18): `Share` submits a `share` job; progress arrives as `JobEvent`s, for which the connection must have sent `JobEvents`.

There is no `AiQuery` and no `AiCancel`: the in-app panel that answered in the window is gone, and `AiOpen` starts the tool in a terminal instead.

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
→ {"id":2,"type":"JobEvents"}
← {"id":2,"ok":{}}
→ {"id":3,"type":"Submit","op":{"op":"trash","items":["file:///home/david/wallpapers.zip"]}}
← {"id":3,"ok":{"job":41}}
← {"event":"JobEvent","job":{"id":41,"op":"trash","state":"done","done":1,"total":1,"bytes":0,"bytesTotal":0,"title":"Move wallpapers.zip to Trash","error":null,"undoable":true,"name":"wallpapers.zip","count":1,"isDir":false,"direction":"local","phase":"running","cancelling":false,"current":null,"rate":0,"result":null,"hidden":false}}
← {"event":"Toast","job":41,"text":"Moved wallpapers.zip to Trash","undoable":true}
→ {"id":4,"type":"Undo"}
← {"id":4,"ok":{"job":42}}
```

## Compatibility rules

- New fields may be added to any object; clients ignore unknown fields.
- New request types, events and error codes may be added within a version.
- Removing or renaming a field, or changing a type, bumps `version`.
