# 22 — Access heat map

Builds on: `01-daemon-and-listing.md` (`Meta`, sort roles), `21-view-memory-and-columns.md` (optional list columns), `20-settings.md` (General page).

## Goal

A glance at a folder shows what was touched recently. The **Accessed** column renders each file's last access time as a relative phrase over a heat swatch, so the files you were working on stand out from the ones you have not opened in months, and the column sorts so "most recently used" is one click away.

## Rendering

- **Text**: a relative phrase, not a date: "just now" (under 45 s), "5 min ago", "3 h ago", "2 days ago" (up to two weeks), "3 weeks ago", "5 months ago", "2 years ago". A dash when the time is unknown.
- **Heat**: a rounded swatch behind the text in the theme's accent colour. Alpha is `0.05 + 0.45·t²` where `t = 1 − log(hours) / log(hours in a year)`, clamped to 0..1: fully saturated for the last hour, still clearly warm for today, faint for last month, gone at about a year. The log scale is what makes the difference between "today" and "this week" visible while a year and two years look the same, which is how people think about recency.
- **Selection**: the swatch hides on a selected row so the selection colour stays readable.
- **Sorting**: `Sort { role: "atime" }`; the header shows the arrow like any other column. Sorting by access time needs `Meta` for every row, so it goes through `Enrich` like size and modified.
- **Icon and columns views**: no swatch in 0.1.0. The inspector's General tab shows the same relative phrase as an "Accessed" field.

## Where the time comes from

- `Meta.atime` (ms since the epoch, 0 = unknown), added to the metadata every listing already carries. Locally it comes from the same `statx` call as size and mtime with `STATX_ATIME` in the mask, so the column costs nothing extra to fill. Remote plugins may report `atime` in `Meta`; SFTP does, FTPS and the device plugins do not, and those rows show a dash.
- **The `relatime` caveat**, stated next to the column switch in Settings: Linux's default mount option updates atime at most once a day unless the file changed since it was last read, so on a stock Omarchy install the heat is accurate to the day. `strictatime` in `/etc/fstab` gives minute accuracy at a small write cost; `noatime` makes the column meaningless and the daemon detects it from `/proc/self/mounts` and reports `atimeSupport: "noatime" | "relatime" | "strictatime"` in `Volumes` so the UI can say so.

## kiki's own access log (opt-in)

For heat that means "what I opened in kiki" regardless of mount options:

- The daemon records an open in `~/.local/share/kiki/access.log` whenever a file is opened from kiki (`Open`, Open with…, Open in…, the code viewer, a share, a Jarvis query): `unix_ms\tURI`, appended, one line per open, compacted to the newest entry per URI when it passes 50,000 lines.
- With the setting **Heat source** = "kiki opens" the column takes its time from the log and falls back to `atime` for files kiki never opened, marked with a hollow swatch so the two sources are distinguishable. "Filesystem" (the default) uses `atime` only.
- The log is local, never leaves the machine, and "Clear access log" on the General page empties it.

## Settings

General page: the **Accessed** column switch (from plan 21), **Heat source** (Filesystem / kiki opens), the `relatime` note with the detected mode for the current volume, and **Clear access log**.

## Protocol

- `Meta.atime` in every row and `Stat` reply (plan 01).
- `Sort { role: "atime" }`.
- `Volumes` items gain `atimeSupport`.
- `AccessLog { uri } -> { opened: u64 }` and `ClearAccessLog {}`; the daemon appends to the log inside `Launch`, `OpenIn`, `OpenText` and `Share`.

## Verification

- `Format.relative` and `Format.heat` unit tests: the phrase boundaries above; alpha is 0.5 at one hour, about 0.28 at a day, about 0.05 at a year, and never above 0.5 or below 0.05.
- Reading a file with `cat` on a `strictatime` mount changes its swatch to full within one listing refresh; on `relatime` a second read within the day does not.
- With Heat source = kiki opens, opening a file through Open with… updates its swatch immediately while `cat` from a terminal does not; Clear access log returns the column to filesystem times.
- Sorting by Accessed on a 10,000-entry folder finishes within the plan-01 `Enrich` budget.
