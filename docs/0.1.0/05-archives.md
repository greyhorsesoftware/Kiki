# 05 — Archives

Builds on: `04-operations-and-undo.md`.

## Goal

Compress selections and extract archives as jobs, without buffering whole archives in memory, undoable like any other operation.

## Design

- Formats: `zip`, `tar`, `tar.gz`, `tar.xz`, `tar.zst`, `tar.bz2`, `7z`. All through `bsdtar` (libarchive, in Arch base), spawned like `ffmpeg`, so no compression crates enter the daemon; progress is bsdtar's verbose output, one line per entry.
- **Compress…** opens a small dialog: archive name, format, destination (defaults to the current folder). Streams the selection into the archive; progress is bytes read.
- **Extract here** extracts next to the archive into a folder named after it when the archive has more than one top-level entry; **Extract to…** asks for a folder. Path traversal (`../`) entries are refused and the job fails before writing.
- Inverse: compress → delete the archive; extract → delete the extracted tree (recorded as the list of top-level paths created).
- The inspector (plan 03) shows an archive's member list as its preview, read from the central directory only.

## Verification

- Round-trip a 1 GB tree: compress, extract elsewhere, `diff -r` identical, peak RSS under 100 MB.
- A crafted zip with `../evil` fails with a typed error and creates nothing.
- Undo after compress removes only the archive; undo after extract removes only what extraction created.
