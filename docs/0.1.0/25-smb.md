# 25 — SMB locations

Builds on: `06-remote-locations.md` (plugin processes, the location plugin contract, keyring, sessions), `04-operations-and-undo.md` (jobs), `08-mirror.md` (detectors), `24-mirror-view.md` (remote locations open in Mirror view), `09-omarchy-integration.md` (scheme handlers).

Mockup: the Add-location dialog gains an **SMB** tab; same layout as `AddLocationSFTP.dc.html`.

> **Not in the default build.** `plugin::LOCATION_KINDS` and the workspace's `default-members` ship only `sftp` and `ftps` today; this plan's plugin builds with `-p` and needs its entry added back to both lists (see `06-remote-locations.md`).

**WebDAV** — removed from 0.1.0 (2026-09-21). The `dav` kind was the same binary installed under a second name, and it was never opened against a WebDAV server; a kind offered in the Add-location dialog that nobody has connected once is a promise this release cannot keep. 0.1.0 installs `kiki-plugin-gio` as `kiki-plugin-smb` and nothing else. The `dav` arms stay in the plugin's source beside the `afp` ones — the kinds this binary would answer for and no build runs it under — so putting WebDAV back is `plugin::LOCATION_KINDS`, `default-members`, the PKGBUILD's install line and an `optdepends` on `gvfs-dnssd`. A `dav` location saved by an earlier build stays in `locations.toml` and refuses to open with "WebDAV is not in this version" (`plugin::no_such_kind`, `location_lifecycle.rs`).

**SMB1 (NT1)** — not supported, and will not be (owner, 2026-09-21). SMB1 is unauthenticated at the transport, unencrypted, and withdrawn by everyone who wrote it; kiki speaks **SMB2 or later**. kiki does not enforce this itself and does not need to: the mount goes through `gvfsd-smb` and libsmbclient, whose own `client min protocol` is SMB2_02 on current Samba, and a shared session daemon cannot be given a different minimum per calling process anyway. So kiki passes **no protocol option at all** — nothing in the tree mentions `NT1`, `min protocol` or `smb1` except to refuse it — and a user who has lowered the minimum in their own `smb.conf` has decided that for themselves. What kiki does do is name the failure: an NT1-only server fails in negotiation, and rather than pass Samba's `NT_STATUS_CONNECTION_RESET` on as a network error, the plugin says **"This server only offers SMB1, which kiki does not support."** (`smb1_only`, matched narrowly on the message text and unit-tested; see Errors below).

## Goal

Windows shares, NAS boxes and Samba servers (`smb://nas/media`) browse, transfer and mirror like SFTP and FTPS, through a location plugin built on GIO and the GVfs that Omarchy already runs, with nothing SMB-specific in the daemon or the shell. Shares on the LAN are discovered so adding one is mostly clicking, and the same plugin could cover WebDAV and AFP with a smaller form each — neither is in 0.1.0.

**Status**: generated (`plugins/kiki-plugin-gio`, unverified until Omarchy: it links GLib and needs `gvfsd`); the `browse` field kind, the `Browse` request, the `hidden` flag and `available` in `Describe` are built and tested.

## Client: GIO and GVfs, not libsmbclient

Omarchy ships GVfs (Nautilus depends on it), so the session already runs `gvfsd` and its `gvfsd-smb` backend, itself built on libsmbclient. kiki uses that stack through **GIO** rather than linking Samba into its own process:

- **One plugin, several schemes.** The binary is `kiki-plugin-gio` and it reads its scheme from `argv[0]`; 0.1.0 installs it as `kiki-plugin-smb` only (`kiki-plugin-dav` and `kiki-plugin-afp` are what putting WebDAV or AFP back would install). `Describe` reports the scheme's own form (this plan specifies the SMB one). SFTP and FTPS keep their native plugins, which are already built and mock-tested.
- **Mounting**: `Connect` calls `g_file_mount_enclosing_volume` on `smb://host/share/` with a `GMountOperation` that answers the password prompt from the location's config and keyring secret (`Password`), passes through when a Kerberos ticket is present (`Kerberos ticket`), or answers anonymous (`Guest`). A wrong password surfaces as `Auth` with the server's message; an unknown share as `NotFound` on the `share` field; a refused connection as `Network` naming the host. The mount is shared with the rest of the desktop, so a share mounted in kiki also appears in GTK file choosers.
- **Listing**: `enumerate_children` with `standard::name,standard::type,standard::size,standard::is-hidden,time::modified,time::modified-usec,unix::mode,owner::user,owner::group`, 512 entries per batch, fills `Meta` inline (`metaInScan: true`); SMB's directory query already carries the attributes, so this is one round trip per batch.
- **Reads and writes**: `g_file_read` / `g_file_replace` streams with 1 MiB buffers; `partialRead: true` through `seek` on the input stream. `pipelining: false`: GIO is synchronous per call and throughput comes from request size. A job session mounts nothing new; it reuses the mount and runs on its own thread, so a transfer never blocks a listing.
- **Metadata**: `set_mtime` through `g_file_set_attribute` `time::modified`; `mode: false` (DOS attributes are not a mode).
- **Discovery**: enumerating `network:///` lists hosts GVfs found through mDNS and WS-Discovery; enumerating `smb://host/` lists the shares. No Avahi or WS-Discovery code in kiki.
- **Dependencies**: the `gio` and `glib` crates in that one plugin; `gvfs-smb` as an optional package dependency. Without a running `gvfsd` the plugin's `Describe` reports `available: false` with "GVfs is not running" and the dialog tab shows that instead of the form.
- **Trade-offs recorded**: every call crosses D-Bus to `gvfsd-smb`, which is invisible for listings and metadata and a small cost on large transfers compared with direct libsmbclient; the FUSE view GVfs exposes under `/run/user/<uid>/gvfs/` can give the daemon a local path for big copies later. A pure-Rust or direct-libsmbclient client can replace the backend behind the same plugin protocol if GVfs ever becomes a problem.

## Form (`Describe`)

| Field | Kind | Notes |
|---|---|---|
| Name | text, required | the URI authority |
| Host | text, required | name or address; **Browse…** next to it lists discovered hosts |
| Share | text, required | **Browse shares…** enumerates the server's shares once host and credentials are filled (GIO enumerates `smb://host/`) |
| Authentication | select | Password (default), Kerberos ticket, Guest |
| Username | text | required for Password |
| Password | password | keyring; secret field |
| Domain / workgroup | text | default `WORKGROUP`; also accepts `DOMAIN\user` in Username |
| Remote path | path | inside the share, default `/` |
| Local path | path | for Mirror view |

There is **no Encryption field**. This plan specified one (Auto / Required / Off, "Required refuses SMB1 and unsigned sessions") and it was never built: the mount goes through `gvfsd-smb`, a session daemon shared with the rest of the desktop, and there is no per-process way to hand it a protocol or encryption minimum. What is built instead is the line above — SMB2 or later, because that is libsmbclient's own default, and an SMB1-only server refused by name. A field offering three settings, none of which reached the server, would have been worse than the absence of one.

Remote URI: `smb://<name>/<path>` where `<name>` is the location; the host and share live in config, so renaming a share on the NAS is a config edit, not a new location. A pasted `smb://host/share/path` from another application (the `x-scheme-handler/smb` registration below) opens the dialog prefilled when no location matches host and share.

## Capabilities and behaviour

- `Capabilities`: `trash: false`, `setMtime: true` (`time::modified` attribute), `mode: false` (DOS attributes are not a mode; the inspector's Permissions tab shows Read-only, Hidden, Archive as three switches later, not in this plan), `realDirs: true`, `digestKind: null`, `separator: "/"`, `partialRead: true` (stream seek), `fastScan: "gio"` (informational: attributes arrive with the listing).
- **Listing**: attributes arrive with the names (see the client section), so `metaInScan: true` costs nothing. Dot-files are not the hidden convention on SMB; the plugin maps GIO's `standard::is-hidden` (the DOS Hidden attribute) to `hidden: true` in `Meta` so the daemon's hidden filter (plan 23) honours it.
- **Reads and writes**: 1 MiB stream buffers; `pipelining: false`. The job session runs on its own thread over the same mount, so a transfer never blocks a listing.
- **Rename, delete, mkdir**: direct. Delete of a non-empty folder is `NotEmpty` as everywhere.
- **Mirror**: detector `sizeMtime` both ways; SMB keeps 100 ns timestamps and the engine's clock-offset heuristic handles NAS clocks that drift. `Digest` is never offered.
- **DFS**: referrals are followed by the GVfs backend transparently; the listing shows the target's entries.
- **Errors** (from `GIOErrorEnum`): `PERMISSION_DENIED` at mount time → `Auth`, after mount → `Denied`; `NOT_FOUND` / `NOT_MOUNTED` for the share → `NotFound` on the `share` field at Connect; `HOST_NOT_FOUND`, `HOST_UNREACHABLE`, `CONNECTION_REFUSED`, `TIMED_OUT` → `Network` naming the host.
- **A server that speaks only SMB1** is `Network` with the words "This server only offers SMB1, which kiki does not support." GIO gives negotiation failures no code of their own — libsmbclient reports them through several errnos and they arrive as a plain `Failed` — so the plugin reads the message, which is Samba's own: "Protocol negotiation to server nas (for a protocol between SMB2_02 and SMB3_11) failed: NT_STATUS_CONNECTION_RESET", inside gvfsd-smb's "Failed to mount Windows share: …". The match requires the word "negotiat" **and** one of SMB1, NT1, `NT_STATUS_CONNECTION_RESET`, `NT_STATUS_CONNECTION_DISCONNECTED` or `NT_STATUS_INVALID_NETWORK_RESPONSE`, so a connection reset that is only a connection reset keeps its own words; a server whose failure says less than that falls through to the ordinary network error, which is the safe way round. Unit-tested against both lists of strings (`smb1_only`). **Read from Samba's source, not produced here**: there is no `smbd` on the development machine to offer NT1, so the strings want one check against a real one in phase 8.

## Discovery

- **Hosts**: enumerate `network:///` through GIO; GVfs merges mDNS (`_smb._tcp`) and WS-Discovery announcements and returns one entry per host with its name and, when known, the model. Triggered by the **Browse…** button, five-second cap.
- **Shares**: enumerate `smb://host/` after authentication, minus `IPC$`, `print$` and `<letter>$` admin shares.
- A later plan can put discovered hosts under a **Network** sidebar header; this plan stops at the dialog.

## Daemon and shell touches (all generic)

- `x-scheme-handler/smb` joins the MIME types the Omarchy integration sets (`integrate.rs`), so `smb://` links from browsers open kiki.
- The Add-location dialog already renders any plugin's form; the two Browse… buttons are a new field kind `browse` with a plugin request `Browse { field, config, secrets } -> { options: [{ value, label }] }` added to `API-PLUGIN.md` as optional (a plugin without it has no button). SFTP can use the same kind later for known hosts.
- `Meta.hidden` (optional bool) joins the plugin `Meta` type; the daemon's hidden filter treats it like a leading dot.
- Nothing else changes: URIs resolve by scheme and authority, listings go through the string pool and windows, transfers are jobs, Mirror view opens the location like any other.

## Packaging

`kiki-plugin-gio` is built like the other plugins and installed once, under the name `kiki-plugin-smb` (a kind is the suffix of the name a plugin is run by). `optdepends`: `gvfs-smb: SMB (Windows share) locations`. WebDAV and AFP would each be a second install of the same binary under their own name, with `gvfs-dnssd` for WebDAV; neither is in 0.1.0. The plugin's `Describe` reports `available: false` with the reason when `gvfsd` is not reachable on the session bus, and the dialog tab shows that instead of the form.

## Verification

- Contract tests against a local `smbd` (Arch `samba`, temp `smb.conf` with one share, one user, a guest share, `server min protocol = SMB2` — it stays, and is what kiki expects of a server — bound to a high port with `smb ports`) under `dbus-run-session` with `gvfsd` and `gvfsd-smb` started from the session bus: list, stat, mkdir, rename, delete, upload, download with offset, set mtime; the hidden attribute maps to `hidden`; wrong password is `Auth`; a share name that does not exist is `NotFound` on `share`; **a second server with `server max protocol = NT1` is refused with a plain message — "This server only offers SMB1, which kiki does not support." — and not with Samba's status code**; with `gvfsd` stopped, `Describe` reports `available: false`. CI runs these on the Arch container; on macOS the suite is `#[ignore]`.
- Mirror scan of a 5,000-file share completes in one enumeration batch per 512 entries and produces the same map as a walk of the share's FUSE view (diffed).
- Discovery lists the test `smbd` (announced through Avahi in the container) within five seconds; Browse shares… lists the test share and not `IPC$`.
- `xdg-open smb://nas/media/photos` opens the dialog prefilled with host `nas` and share `media` when no location matches.
