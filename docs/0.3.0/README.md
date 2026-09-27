# 0.3.0

The plans for the release after 0.2.x, in no settled order:

1. `01-daemon-on-demand.md` — **the engine belongs to the window**: `kikid` started by the window and gone with it, no systemd units, and the D-Bus listener (already its own binary) split off as the one on-demand piece — bus-activated, it starts a window rather than the bus starting an engine to start a window. With it the restructure: opening a file as one verb with one rule table (`open.rs`), `server.rs` as a router over per-area handlers with one way to answer slowly. **Planned and built 2026-09-26** — L1–L6 landed, the manual pass done on the owner's login; `API-DELTA.md` records what the protocol lost and gained (`Launch` for everything opened elsewhere; the four open verbs, `ChooserResult`, `ShowItems`/`ShowChooser` gone).
2. `02-omarchy-store.md` — kiki measured against `docs/knowledge/develop-for-omarchy.md` and the work to close the gap: a PKGBUILD that builds anywhere (a tagged tarball and a checksum, not `${startdir}/..`), real checksums in `kiki-bin`, `namcap` in CI, the dependency audit, the AUR, and the store pull request. Follows `01`, which removes the systemd units the checklist objects to. **Planned 2026-09-26**, about 3 days.
3. `API-DELTA.md` — the 0.3.0 protocol changes, for whoever reads the wire.
