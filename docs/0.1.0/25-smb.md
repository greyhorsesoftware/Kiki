# 25 — SMB locations

Builds on: `06-remote-locations.md` (plugin processes, the location plugin contract, keyring, sessions), `04-operations-and-undo.md` (jobs), `08-mirror.md` (detectors), `24-mirror-view.md` (remote locations open in Mirror view), `09-omarchy-integration.md` (scheme handlers).

Mockup: the Add-location dialog gains an **SMB** tab; same layout as `AddLocationSFTP.dc.html`.

## Goal

Windows shares, NAS boxes and Samba servers (`smb://nas/media`) browse, transfer and mirror like SFTP and FTPS, through a third location plugin, `kiki-plugin-smb`, with nothing SMB-specific in the daemon or the shell. Shares on the LAN are discovered so adding one is mostly clicking.

## Client library

SMB2 and SMB3 with NTLMv2, Kerberos, signing, encryption and DFS referrals are a large surface, and no pure-Rust client is complete enough to ship on. The plugin links **libsmbclient** from Samba, the same library GNOME's gvfs and KDE's kio use, through the `pavao` crate (a safe wrapper; falls back to hand-written `extern "C"` declarations in the style of the device plugins if the crate lags behind Samba). The dependency stays inside the plugin binary; `samba` (Arch package `smbclient`) becomes a runtime dependency of that one plugin, listed as optional in the PKGBUILD like the device libraries. A pure-Rust client can replace it later behind the same plugin protocol.

Consequences of libsmbclient:

- One `SMBCCTX` per session, never shared across threads; the plugin serialises each session behind a mutex like FTPS does, while separate sessions (browse and job) run concurrently under the concurrent SDK.
- Authentication is a callback: the plugin answers it from the location's config and the keyring secret; a wrong password surfaces as `Auth` with the server's message, a missing share as `NotFound`, an ACL denial as `Denied`.
- Kerberos works when a ticket exists (`KRB5CCNAME`); the form's **Authentication** select has "Password", "Kerberos ticket" and "Guest".

## Form (`Describe`)

| Field | Kind | Notes |
|---|---|---|
| Name | text, required | the URI authority |
| Host | text, required | name or address; **Browse…** next to it lists discovered hosts |
| Share | text, required | **Browse shares…** enumerates the server's shares once host and credentials are filled (libsmbclient lists `smb://host/`) |
| Authentication | select | Password (default), Kerberos ticket, Guest |
| Username | text | required for Password |
| Password | password | keyring; secret field |
| Domain / workgroup | text | default `WORKGROUP`; also accepts `DOMAIN\user` in Username |
| Remote path | path | inside the share, default `/` |
| Local path | path | for Mirror view |
| Encryption | select | Auto (default), Required, Off; Required refuses SMB1 and unsigned sessions |

Remote URI: `smb://<name>/<path>` where `<name>` is the location; the host and share live in config, so renaming a share on the NAS is a config edit, not a new location. A pasted `smb://host/share/path` from another application (the `x-scheme-handler/smb` registration below) opens the dialog prefilled when no location matches host and share.

## Capabilities and behaviour

- `Capabilities`: `trash: false`, `setMtime: true` (`smbc_utimes`), `mode: false` (DOS attributes are not a mode; the inspector's Permissions tab shows Read-only, Hidden, Archive as three switches later, not in this plan), `realDirs: true`, `digestKind: null`, `separator: "/"`, `partialRead: true` (`smbc_lseek`), `fastScan: "readdirplus" | "none"`.
- **Listing**: SMB `QUERY_DIRECTORY` returns names with full attributes, so `metaInScan: true` costs nothing: the plugin uses `smbc_readdirplus2` (Samba 4.12+) and fills `Meta` (size, mtime with 100 ns precision, hidden and read-only flags) in one round trip per 64 KiB of entries; older Samba falls back to `readdir` plus one `stat` per entry, reported as `fastScan: "none"`. Dot-files are not the hidden convention on SMB; the plugin maps the DOS Hidden attribute to a leading-dot-equivalent `hidden: true` in `Meta` so the daemon's hidden filter (plan 23) honours it.
- **Reads and writes**: 1 MiB requests (SMB2 large MTU); libsmbclient is synchronous so throughput comes from request size rather than pipelining (`pipelining: false`). A job session is separate from the browse session, so a transfer never blocks a listing.
- **Rename, delete, mkdir**: direct. Delete of a non-empty folder is `NotEmpty` as everywhere.
- **Mirror**: detector `sizeMtime` both ways; SMB keeps 100 ns timestamps and the engine's clock-offset heuristic handles NAS clocks that drift. `Digest` is never offered.
- **DFS**: referrals are followed by libsmbclient transparently; the listing shows the target's entries.
- **Errors**: `NT_STATUS_LOGON_FAILURE` → `Auth`; `ACCESS_DENIED` → `Denied`; `BAD_NETWORK_NAME` → `NotFound` on the `share` field at Connect; `CONNECTION_REFUSED`/`HOST_UNREACHABLE` → `Network` naming the host; `NOT_SUPPORTED` when the server only speaks SMB1 and Encryption is Required → `Invalid` on the encryption field with a one-line explanation.

## Discovery

- **Hosts**: `avahi-browse -rpt _smb._tcp` (mDNS, what macOS and most NAS boxes announce) and WS-Discovery on 3702 (what Windows 10+ announces), merged, five-second scan on demand when the Browse… button is pressed; NetBIOS browsing is not attempted. Results carry host, address and a model hint when Avahi gives one.
- **Shares**: libsmbclient's directory listing of `smb://host/` after authentication, minus admin shares (`C$`, `IPC$`, `print$`).
- A later plan can put discovered hosts under a **Network** sidebar header; this plan stops at the dialog.

## Daemon and shell touches (all generic)

- `x-scheme-handler/smb` joins the MIME types the Omarchy integration sets (`integrate.rs`), so `smb://` links from browsers open kiki.
- The Add-location dialog already renders any plugin's form; the two Browse… buttons are a new field kind `browse` with a plugin request `Browse { field, config, secrets } -> { options: [{ value, label }] }` added to `API-PLUGIN.md` as optional (a plugin without it has no button). SFTP can use the same kind later for known hosts.
- `Meta.hidden` (optional bool) joins the plugin `Meta` type; the daemon's hidden filter treats it like a leading dot.
- Nothing else changes: URIs resolve by scheme and authority, listings go through the string pool and windows, transfers are jobs, Mirror view opens the location like any other.

## Packaging

`kiki-plugin-smb` is built and installed like the other plugins; `smbclient` (the Samba client library package) is an `optdepends` entry: "SMB locations". The plugin's `Describe` reports `available: false` with the reason when the library is missing, and the dialog tab shows that instead of the form.

## Verification

- Contract tests against a local `smbd` (Arch `samba`, temp `smb.conf` with one share, one user, guest share, and `server min protocol = SMB2`): list, stat, mkdir, rename, delete, upload, download with offset, set mtime; the hidden attribute maps to `hidden`; wrong password is `Auth`; a share name that does not exist is `NotFound` on `share`; Encryption Required against `server smb encrypt = off` is `Invalid` on the encryption field. CI runs these on the Arch container (`smbd` needs no root when bound to a high port with `smb ports`); on macOS the suite is `#[ignore]`.
- Mirror scan of a 5,000-file share completes in one listing round trip per directory with `readdirplus`; the same scan over the `readdir`+`stat` fallback is slower but produces the identical map (diffed).
- Discovery finds a Samba instance announced by Avahi on the test machine within five seconds; Browse shares… lists the test share and not `IPC$`.
- `xdg-open smb://nas/media/photos` opens the dialog prefilled with host `nas` and share `media` when no location matches.
