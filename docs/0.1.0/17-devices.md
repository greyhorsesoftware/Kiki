# 17 — Devices (phones and cameras)

Builds on: `06-remote-locations.md` (plugin processes, plugin API), `01-daemon-and-listing.md` (windows, previews), `03-inspector.md` (thumbnails), `04-operations-and-undo.md` (jobs).

**Status**: specified, not yet generated. The plugins link C libraries that are not available on the planning machine; build them first on Omarchy with `libmtp`, `libgphoto2` and `libimobiledevice` installed.

## Goal

A plugged-in Android phone (MTP), iPhone (AFC) or camera (PTP) appears in a **Devices** section of the sidebar within a second, browses like any other location with thumbnails and previews, supports copy in both directions and delete where the protocol allows, and disappears cleanly on unplug or eject. Each protocol is a plugin process; the daemon adds hotplug detection and nothing device-specific.

## Plugins

Three binaries following `API-PLUGIN.md`, each linking its C library so those dependencies never enter kikid. All report `trash: false`, `setMtime: false`, `mode: false`, and a new capability `partialRead: false` (see below).

**`kiki-plugin-ptp`** (cameras, and Android in camera mode) — `libgphoto2` through the `gphoto2` crate.
- Listing via the camera's folder tree (`store_00010001/DCIM/...`), one folder per `Scan`, with `Meta` from the file info (size, capture time as mtime).
- `Read` streams a file; `Delete` works on most cameras; `Write`, `Mkdir`, `Rename` return `Unsupported` unless the camera reports the capability.
- **Thumbnails**: PTP has a native get-thumbnail operation, so `Thumbnail` for a photo is a few KB from the camera rather than the full image. The plugin answers a new optional plugin message `Thumb { location, path } -> binary` and the daemon uses it before its own pipeline.
- Cameras that sleep drop the connection; the plugin reconnects once on the next request and reports `Network` if that fails.

**`kiki-plugin-mtp`** (Android and other MTP devices) — `libmtp` via FFI.
- Connect must not enumerate the device: `libmtp`'s default open walks every object, which takes minutes on a phone with thousands of photos. The plugin opens with the uncached variant and lists one folder at a time with the per-folder call, so phase 1 stays proportional to the folder.
- Storage volumes (internal, SD card) are the root's children.
- `Read` and `Write` are whole-file transfers; `Rename`, `Delete`, `Mkdir` work; `SetMtime` is unsupported (MTP records the modification date at upload from the sent metadata, which the plugin fills from the source when the daemon provides it in `Write`).
- One client at a time: a second `Connect` for a `job` role shares the single session with a mutex rather than opening another, and the plugin serialises transfers.
- Object handles are cached per path for the session so repeated stats do not re-list.

**`kiki-plugin-afc`** (iPhone, iPad) — `libimobiledevice` through the `rusty_libimobiledevice` bindings, with the `usbmuxd` daemon as a package dependency.
- Pairing: on first `Connect` the device shows its Trust prompt; the plugin returns `Invalid` with `field: "trust"` and a message, and the daemon shows "Tap Trust on the device, then retry" with a Retry button. A locked device returns `Auth` with "Unlock the device".
- Root is the media area (`DCIM`, `Downloads`, `Books`, recordings), the only part AFC exposes without a jailbreak; the location's tooltip says so.
- Read, write, delete, mkdir and rename work within that area; stat gives size and mtime.
- App document sharing (per-app folders) is a later addition via the house-arrest service.

**`partialRead: false`**: none of the three can serve a byte range, so the daemon never issues range reads for previews; it fetches the whole file into the preview cache once and previews from there, and it shows an in-progress state for files over 10 MB.

## Detection and the Devices section

- **Hotplug**: a thread in kikid listens on the kernel uevent netlink socket (`AF_NETLINK`, `NETLINK_KOBJECT_UEVENT`) through `rustix`, no libudev. On `add` and `remove` of a USB interface it reads `/sys/bus/usb/devices/<id>/` attributes and classifies: interface class `06`/subclass `01` is PTP; an MTP interface string or the Microsoft OS descriptor marker is MTP; Apple's vendor id `05ac` with a `usbmuxd` socket present is AFC. Unknown USB devices are ignored. At startup the same classification runs over the existing tree.
- Each detected device becomes a **transient location** with scheme `ptp://`, `mtp://` or `afc://` and authority `<vendor>-<model>-<serial>`, listed under a **Devices** header in the sidebar that appears only while at least one device is present. Transient locations are not written to `locations.toml`; a device seen before keeps its user-given display name from `devices.toml` (name and last-seen only).
- **Eject** (context menu, `Ctrl+E` with the device selected, or the eject icon on hover) disconnects the session and, for MTP and AFC, sends the protocol's close so the phone stops showing "connected to computer". Unplugging without eject during a transfer fails that job with `Network` and removes the entry.
- Copy between a device and anywhere else is a plan-04 job like any transfer; the journal records only what can be undone (a copy to the device can be undone by deleting; a copy from the device likewise).
- If another virtual-filesystem daemon (gvfs, kio) has already claimed the device, the plugin reports `Busy` with the process name and the sidebar entry shows it instead of failing silently.
- Permissions: the `libmtp`, `libgphoto2` and `usbmuxd` packages ship udev rules that grant the seated user access through logind; the PKGBUILD depends on them and no group changes are needed.

## Protocol additions

Plugin API: `Capabilities` gains `partialRead: bool`; optional `Thumb { location, path } -> binary frames` (a plugin that lacks it returns `Unsupported`); `Write` gains an optional `mtime` the plugin may apply at upload.

Daemon API: `Devices -> { devices: [{ uri, kind: "ptp" | "mtp" | "afc", name, vendor, model, connected: bool, busy: string | null }] }`, `Eject { uri } -> {}`, events `DeviceAdded { device }`, `DeviceRemoved { uri }`.

**IPC added**: `eject(uri)`.

**Mockup**: add a Devices section to the sidebar on `Main.dc.html` (a phone and a camera, eject icon on the hovered row) before building.

## Verification

- Plugging in a camera in PTP mode, an Android phone in file-transfer mode, and an iPhone: each appears under Devices within 1 s (uevent to sidebar), with the right kind and name; unplugging removes it within 1 s.
- Android with 8,000 photos in DCIM: `Connect` completes under 2 s (no full enumeration); the DCIM listing paints the first window within 500 ms; thumbnails arrive as rows enter the window.
- Camera: thumbnails for a folder of 500 RAW+JPEG pairs come from the PTP thumbnail operation (counted) and no full file is fetched for the icon view.
- iPhone before Trust: the sidebar shows the retry message; after tapping Trust, Retry connects and lists DCIM.
- Copy 200 photos from each device to local and 20 back to the phone; the journal undoes the copy to the phone by deleting; sizes and dates match on both sides.
- Unplug mid-transfer: the job fails with `Network`, the entry disappears, and the daemon and plugin are still responsive (Ping).
- Eject on MTP: the phone's notification changes to disconnected within 2 s.
- A second `Connect` on MTP with role `job` does not open a second session; a transfer and a listing interleave without error.
