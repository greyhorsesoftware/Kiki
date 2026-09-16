# 09 — Omarchy integration

Builds on: `01-daemon-and-listing.md` (the daemon), `02-shell-and-views.md` (the window).

Mockup: `OpenDialog.dc.html`.

## Goal

kiki is Omarchy's file manager: `xdg-open` on a folder launches it, browsers and chat apps can "Show in folder", and every app's Open and Save dialogs are kiki's.

## Design

- **Desktop entry**: `org.kiki.App.desktop` (the id must equal the bus name for D-Bus activation to work) with `MimeType=inode/directory;` and `DBusActivatable=true`. No `x-scheme-handler/file`: `xdg-open` does not route `file:` through scheme handlers. Install runs `xdg-mime default org.kiki.App.desktop inode/directory`.
- **D-Bus activation**: kikid owns `org.kiki.Daemon`; `kiki` the window owns `org.kiki.App` with `Activate`, `Open(uris)`. Launching with a path opens or focuses a window at it (Hyprland focus via `hyprctl dispatch focuswindow`).
- **URIs everywhere**: the `kiki` binary accepts any URI (`kiki sftp://homelab/srv`), the desktop entry registers `x-scheme-handler/sftp` and `x-scheme-handler/ftps` so `xdg-open sftp://…` lands in kiki, and every D-Bus entry point below passes URIs straight to `open(uri)`.
- **Show in folder**: implement `org.freedesktop.FileManager1` (`ShowItems`, `ShowFolders`, `ShowItemProperties`) on the daemon via `zbus`. `ShowItems` opens a window at the parent and selects the file; `ShowItemProperties` opens it with the inspector on.
- **Portal FileChooser**: an `org.freedesktop.impl.portal.FileChooser` backend (`OpenFile`, `SaveFile`, `SaveFiles`) in the daemon, with a portal config that routes FileChooser to kiki. The daemon owns the D-Bus interface; the dialog itself is a Quickshell window that the daemon asks the running kiki to show over its IPC (starting kiki if it is not running) and whose result comes back over the socket. Parenting to the caller's window via the portal's `parent_window` handle (`xdg_foreign`) is to be verified against Quickshell early; if unsupported, the dialog opens centred on the caller's monitor via Hyprland. The dialog is the mockup: a trimmed window (Favorites and Locations, breadcrumb, search, list, filter dropdown, Cancel / Open or Save). Filters, multiple selection, directory mode, current folder and suggested name all follow the portal spec. Remote locations are allowed; the daemon streams the file to a temporary local path and returns that URI.
- **Trash favorite** integrates with `org.freedesktop.FileManager1`-aware apps by exposing the standard Trash directory.

## Verification

- `xdg-open ~/` opens kiki at home; `xdg-open file.txt` still opens the file's own handler.
- Firefox's "Show in folder" reveals the download selected.
- A GTK app's Open dialog and a Qt app's Save dialog are kiki's; filters and suggested names arrive correctly; picking a file on the SFTP location returns a readable local URI.
- kiki is not launched twice for two `xdg-open` calls; the second focuses the existing window.
