# 10 — Polish and packaging

Builds on: all previous plans.

## Goal

Ship 0.1.0 as an Omarchy package.

## Design

- **Themes**: verify every Omarchy theme (Tokyo Night, Catppuccin, Nord, Gruvbox, Everforest, Kanagawa, Rose Pine, Matte Black) against the token map; fix any that lack a colour by deriving it in OKLCH from the theme's accent.
- **Cheat sheet**: `?` opens a keybinding overlay generated from the keymap table in plan 02.
- **Performance pass**: re-run the plan-01 benchmarks with all features on; list, icon and columns views must still paint the first chunk within 16 ms.
- **Packaging**: one package, `kiki`, one command, `kiki`. Installed layout:

  | Path | Contents |
  |---|---|
  | `/usr/bin/kiki` | launcher: runs Quickshell on `/usr/share/kiki/shell.qml`, forwards a URI argument to `open(uri)` over IPC if a window is already up |
  | `/usr/bin/kikid` | the daemon |
  | `/usr/lib/kiki/plugins/kiki-plugin-*` | every plugin: locations (`sftp`, `ftps`, later `ptp`, `mtp`, `afc`), services (`dbus`, `highlight`, `jarvis`), share (`share-mail`, `share-messages`, `share-tailscale`, `share-localsend`, `share-airdrop`) |
  | `/usr/share/kiki/` | QML shell, `qml/icons/`, `qmldir` |
  | `/usr/lib/systemd/user/kiki.socket`, `kikid.service` | socket activation; enabled by the post-install hook |
  | `/usr/share/applications/org.kiki.App.desktop` | menu entry, `inode/directory`, `x-scheme-handler/sftp`, `x-scheme-handler/ftps` |
  | `/usr/share/dbus-1/services/org.kiki.App.service` | D-Bus activation for FileManager1 and `Open(uris)` |
  | `/usr/share/xdg-desktop-portal/portals/kiki.portal`, `/usr/share/xdg-desktop-portal/kiki-portals.conf` drop-in | FileChooser backend registration |

  The PKGBUILD builds all Rust binaries from one workspace. Post-install runs `xdg-mime default org.kiki.App.desktop inode/directory`, enables `kiki.socket` for the user session, and restarts `xdg-desktop-portal`; post-remove reverses all three. Depends on `quickshell`, `xdg-desktop-portal`, `ffmpeg`, `libmtp`, `libgphoto2`, `libimobiledevice`, `usbmuxd` (their udev rules grant device access), and a Secret Service provider (`gnome-keyring` unless the open question in `CORE.md` settles otherwise). Optional dependencies: `tailscale`, `kdeconnect`, `signal-cli`, a mail client for `xdg-email`, `owl` and `opendrop` for AirDrop. Third-party plugins are a single file in `~/.local/lib/kiki/plugins/`; the daemon notices new files without a restart. An install line goes into `omarchy`'s package list if adopted.
- **Distribution**: a tag triggers CI to build the package for `x86_64` (Arch container) and `aarch64` (GitHub ARM runner with an Arch Linux ARM container), attach `kiki-<ver>-1-<arch>.pkg.tar.zst` for both to the GitHub release, and push an updated `PKGBUILD` to the AUR for `kiki-bin` with per-architecture sources (repackages the release binary; also a `kiki` source package for people who build). Users install with `yay -S kiki-bin`, which Omarchy's AUR helper supports, and get updates with their normal system update; `pacman -U` on the release file works without the AUR. An own pacman repository (`repo-add` on a static host, one `[kiki]` line in `pacman.conf`) is added only if Omarchy does not adopt kiki into its own repository.
- **Files**: `.github/workflows/ci.yml` (format, lint, tests and a package build on both architectures; the shell harness on x86_64), `.github/workflows/release.yml` (build both packages, publish the GitHub release, update `kiki-bin` on the AUR; needs `AUR_USERNAME`, `AUR_EMAIL`, `AUR_SSH_PRIVATE_KEY` secrets), `packaging/PKGBUILD` (source build, both architectures), `packaging/aur/kiki-bin/PKGBUILD`, and the launcher, units, desktop entry, D-Bus service and portal files under `packaging/`.
- **Docs**: README with install, keymap and the location and mirror workflows.

## Verification

- Fresh Omarchy VM: install the package, log in, `xdg-open ~/` opens kiki, Open dialogs are kiki's, the daemon starts on demand.
- Every theme screenshot reviewed side by side with the mockup palette, as part of the manual checklist in `11-testing.md`.
