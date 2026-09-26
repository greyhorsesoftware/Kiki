# 02 — Into the Omarchy package store

**Status:** planned, 2026-09-26 (owner: "take a look at docs/knowledge/*.md — what needs to
change in this app to adhere and be able to be added to that site").

The checklist is `docs/knowledge/develop-for-omarchy.md` (a community resource, not official
guidance — the official repository and the ArchWiki are what decide). What follows is this
package measured against it, and the work to close the gap. The app is in good shape; nearly
everything below is packaging and delivery.

## Where kiki stands

Passing already: MIT with `LICENSE` installed; no setuid, no odd permissions, no hard-coded
home, credential or local path in a shipped file; uninstall leaves `~/.config/kiki` alone; a
valid desktop entry with a matching `Exec` and an icon in `hicolor/scalable`; **8.7 MB x86_64
/ 8.2 MB aarch64**, stripped, `!debug` (§6's "keep the download small" is the item reviewers
raise first, and it is the one kiki is furthest ahead on); published from GitHub Actions, not
from a contributor's machine.

Failing, in the order a reviewer would hit them:

| | What | Where |
|---|---|---|
| §2 | There is no Omarchy package: no fork, no `pkgbuilds/kiki/`, no `.omarchy/package.json` | — |
| §1 | Neither route is ready: `kiki-bin` was never published to the AUR, and the source package cannot build outside this checkout | `packaging/aur/kiki-bin/PKGBUILD:3` |
| §3, §6 | The source PKGBUILD builds from `${startdir}/..` with no `source=()` and no checksums, so a clean chroot cannot build it at all | `packaging/PKGBUILD:26` |
| §3 | Checksums disabled outright | `packaging/aur/kiki-bin/PKGBUILD:26-27` |
| §4 | An elevated, machine-wide post-install action: `systemctl --global enable kiki.socket`, and `--global disable` on removal | `packaging/kiki.install:11,26` |
| §5 | Install messages tell the user to run `systemctl` by hand | `packaging/kiki.install:21-22` |
| §6 | `namcap` has never been run against the PKGBUILD or the package | — |
| §6 | aarch64 is declared, built and published, but never driven: CI lints, tests and packages it on a native ARM runner, and only x86_64 is run under `cage` | `.github/workflows/ci.yml`, `release.yml` |
| §5 | The desktop entry names more than one main category, so kiki can appear twice in the launcher | `packaging/org.kiki.App.desktop` |
| §3 | Hard dependencies that a reviewer will ask about: `gnome-keyring` (libsecret is the API; which Secret Service implements it is the user's), `git` (the repository overlay is a feature, not a requirement to run), `gvfs` | `packaging/PKGBUILD:15` |

## Decisions

1. **`01-daemon-on-demand.md` goes first.** It deletes the systemd units, and with them §4's
   elevated post-install action and §5's "run systemctl by hand" message. Excusing those in a
   pull request is worse than not having them: the fix is already planned and wanted for its own
   sake. Nothing in this plan is submitted before that lands.
2. **The source package becomes distributable.** `packaging/PKGBUILD` gains a real
   `source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")` and a
   `sha256sums`, and builds in `"$srcdir/Kiki-$pkgver"` instead of `${startdir}/..`. It then
   builds anywhere — a chroot, `omarchy-pkgs`, a stranger's machine — which is what §6's "build
   from a clean checkout" asks and what the store needs. The convenience of building the working
   tree stays, as an explicit opt-in: `KIKI_LOCAL_SRC=1 makepkg` keeps today's behaviour for the
   release checklist, and CI passes it (CI builds the commit it is testing, which has no tag
   yet). A PKGBUILD that silently prefers a nearby checkout is the kind of thing §6 exists to
   catch, so the variable is named in a comment at the top and nowhere else.
3. **Checksums are real.** `kiki-bin`'s `SKIP` goes: the release publishes
   `kiki-<version>-1-<arch>.pkg.tar.zst.sha256` beside each asset already, and the release
   workflow writes the digest into the PKGBUILD as it tags (it verifies both files today, so it
   has them in hand). A checksum is never copied from anywhere but the build that produced the
   asset.
4. **The AUR is the route, when the AUR takes accounts; the local route otherwise.** §1 prefers
   an existing healthy AUR package. `kiki-bin` is written and kept in step; publishing it is a
   step of its own (an AUR account, `ssh-keygen`, a first push) and it unblocks users who prefer
   `yay` whether or not the store ever takes kiki. If the AUR is still closed, `bin/add-package
   kiki --local --scaffold` in a fork of `omarchy-pkgs` carries the source package instead. The
   two are not exclusive and the work below serves both.
5. **The store submission is a pull request and nothing more** (§7, "Important boundary"). No
   signing, no publishing, no release promotion, no opting into a faster ring. The PR says what
   kiki is, where the source comes from, how updates are tracked (the tagged release, a
   checksum per architecture), and exactly what was tested — including, in as many words, that
   **aarch64 is built and unit-tested on a native ARM runner but never driven under a
   compositor**. §6 asks for the gap to be stated rather than papered over.
6. **Dependencies are re-justified, not trimmed by guess.** Each of `gnome-keyring`, `git` and
   `gvfs` is checked by removing it in a container and running the suite: what breaks stays in
   `depends`, what merely loses a feature moves to `optdepends` with the feature named. The
   answer is written into the PKGBUILD's comment block, which is where a reviewer looks.
7. **`namcap` becomes part of the release checklist**, not a thing remembered once. CI runs it
   against the built package and prints its output; a new warning is a line in the log a person
   reads, not a failed build (namcap warns about things that are sometimes right).

## Layers

| | | |
|---|---|---|
| L1 | **A PKGBUILD that builds anywhere**: `source=`/`sha256sums=`, the tagged tarball, `KIKI_LOCAL_SRC=1` for the working tree, `check()` on the unpacked source; build it in a clean `archlinux:latest` container from nothing but the PKGBUILD and confirm the package matches the release's byte for byte where it can. | ½ day |
| L2 | **namcap and the desktop entry**: `namcap` in CI over PKGBUILD and package, its warnings answered; the `Categories` line reduced to one main category; `desktop-file-validate` clean. | ½ day |
| L3 | **The dependency audit**: `gnome-keyring`, `git`, `gvfs` each removed in a container and the suite run; `depends`/`optdepends` corrected, the comment block rewritten. | ½ day |
| L4 | **Checksums**: the release workflow writes each architecture's digest into `packaging/aur/kiki-bin/PKGBUILD` as it tags; `SKIP` gone. | ½ day |
| L5 | **The AUR**: publish `kiki-bin` (account, key, `.SRCINFO`, first push), or record in this plan that the AUR is still closed and the local route is taken. `.SRCINFO` generated by `makepkg --printsrcinfo`, not by hand, and kept current by the release workflow. | ½ day |
| L6 | **The store PR**: fork `omarchy-pkgs`, `bin/add-package kiki` (or `--local --scaffold`), the minimal `.omarchy/package.json`, the repository's dry-run inspected, then a pull request with the testing statement of decision 5. | ½ day |

About **3 working days**, after `01-daemon-on-demand.md`. L1–L4 are worth doing whatever the
store decides: they are what make the package reproducible by anyone.

## Acceptance

- In a clean `archlinux:latest` container, with nothing but `packaging/PKGBUILD` copied in:
  `makepkg -s` fetches the tagged tarball, verifies its checksum and builds a package that
  installs and runs.
- `KIKI_LOCAL_SRC=1 makepkg` still builds the working tree, and CI still packages the commit
  under test.
- `namcap` output is in the CI log and every warning has an answer in the PKGBUILD's comments
  or in this plan.
- `desktop-file-validate` is silent; kiki appears once in the launcher.
- No `systemctl` in `kiki.install`; the post-install message is two lines or fewer and asks for
  nothing (`01-daemon-on-demand.md`).
- Removing `gnome-keyring`, `git` or `gvfs` in a container has a written, tested consequence.
- `kiki-bin`'s checksums are the release's own digests; no `SKIP` anywhere.
- The pull request states what was tested on each architecture, and that aarch64 is not driven.
- `make lint`, `cargo test`, `make test-qml`, `tests/e2e/run.sh` green.

## What this plan does not do

Sign, publish, promote or approve anything; ask for a faster release ring; or treat the
checklist as permission. A maintainer decides whether kiki belongs in the store, and the answer
may be "use the AUR" — which is why L5 stands on its own.
