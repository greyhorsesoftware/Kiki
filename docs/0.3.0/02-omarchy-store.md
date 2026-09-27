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
| §1 | Neither route is ready: `kiki-bin` was never published to the AUR, and the source package cannot build outside this checkout | `packaging/omarchy/PKGBUILD:3` |
| §3, §6 | The source PKGBUILD builds from `${startdir}/..` with no `source=()` and no checksums, so a clean chroot cannot build it at all | `packaging/PKGBUILD:26` |
| §3 | Checksums disabled outright | `packaging/omarchy/PKGBUILD:26-27` |
| §4 | ~~An elevated, machine-wide post-install action: `systemctl --global enable kiki.socket`, and `--global disable` on removal~~ **Gone with the units (01, L3, 2026-09-26)**: the one `systemctl` left is the `--global disable` that sweeps up what 0.2.x enabled, on upgrade and removal, retirable in two releases | `packaging/kiki.install` |
| §5 | ~~Install messages tell the user to run `systemctl` by hand~~ **Gone (01, L3)**: two lines that ask for nothing | `packaging/kiki.install` |
| §6 | `namcap` has never been run against the PKGBUILD or the package | — |
| §6 | aarch64 is declared, built and published, but never driven: CI lints, tests and packages it on a native ARM runner, and only x86_64 is run under `cage` | `.github/workflows/ci.yml`, `release.yml` |
| §5 | The desktop entry names more than one main category, so kiki can appear twice in the launcher | `packaging/org.kiki.App.desktop` |
| §3 | Hard dependencies that a reviewer will ask about: `gnome-keyring` (libsecret is the API; which Secret Service implements it is the user's), `git` (the repository overlay is a feature, not a requirement to run), `gvfs` | `packaging/PKGBUILD:15` |

## Decisions

1. **`01-daemon-on-demand.md` goes first — and has landed (2026-09-26).** It deleted the
   systemd units, and with them §4's elevated post-install action and §5's "run systemctl by
   hand" message. Excusing those in a pull request would have been worse than not having them.
2. **The source package becomes distributable.** `packaging/PKGBUILD` gains a real
   `source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")` and a
   `sha256sums`, and builds in `"$srcdir/Kiki-$pkgver"` instead of `${startdir}/..`. It then
   builds anywhere — a chroot, `omarchy-pkgs`, a stranger's machine — which is what §6's "build
   from a clean checkout" asks and what the store needs. The convenience of building the working
   tree stays, as an explicit opt-in: `KIKI_LOCAL_SRC=1 makepkg` keeps today's behaviour for the
   release checklist, and CI passes it (CI builds the commit it is testing, which has no tag
   yet). A PKGBUILD that silently prefers a nearby checkout is the kind of thing §6 exists to
   catch, so the variable is named in a comment at the top and nowhere else.
3. **Checksums are real.** the store package's `SKIP` goes: the release publishes
   `kiki-<version>-1-<arch>.pkg.tar.zst.sha256` beside each asset already, and the release
   workflow writes the digest into the PKGBUILD as it tags (it verifies both files today, so it
   has them in hand). A checksum is never copied from anywhere but the build that produced the
   asset.
4. **The local route, and only that.** §1 prefers an existing healthy AUR package; there is
   none, and the AUR cannot be done from here (owner, 2026-09-26). So kiki goes in as a package
   the store repository maintains itself: `bin/add-package kiki --local --scaffold` in a fork of
   `omarchy-pkgs`, `pkgbuilds/kiki/` holding the PKGBUILD, `.omarchy/package.json` saying
   `"source": "local"` with the declarative GitHub release provider, so the repository's own
   upstream sync follows kiki's tags. **The store's PKGBUILD is the binary one**, `packaging/omarchy/PKGBUILD` (owner, 2026-09-26):
   it repackages the release asset for each architecture — the bytes CI built and tested — so
   their builders download a small package and need no Rust toolchain, which §5 of the checklist
   prefers; `packaging/PKGBUILD` stays the source package for CI and checkouts. It is named
   `kiki`, not `kiki-bin`: the AUR's `-bin` convention has no place in a store with one package.
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
| L1 | **A PKGBUILD that builds anywhere**: `source=`/`sha256sums=`, the tagged tarball, `KIKI_LOCAL_SRC=1` for the working tree, `check()` on the unpacked source; build it in a clean `archlinux:latest` container from nothing but the PKGBUILD and confirm the package matches the release's byte for byte where it can. **Done 2026-09-26** (the from-nothing build is CI's `source-package` job — no container runtime on the owner's machine — on branches against the commit's own archive with `updpkgsums`, and in the release workflow against GitHub's tag tarball with the digest it has just written; "byte for byte" is not claimed: a package built twice differs in `.BUILDINFO`, so the check is that it builds, installs and carries the version). | ½ day |
| L2 | **namcap and the desktop entry**: `namcap` in CI over PKGBUILD and package, its warnings answered; the `Categories` line reduced to one main category; `desktop-file-validate` clean. **Done 2026-09-26**: `Utility` dropped — `System;FileTools;FileManager;` is one main category (System) and two additional ones the spec ties to it, and `desktop-file-validate` is silent locally and held to silence in CI; namcap runs in CI's x86_64 job and its first output is a thing to read when that run lands (namcap is not packaged on the owner's machine). | ½ day |
| L3 | **The dependency audit**: `gnome-keyring`, `git`, `gvfs` each removed in a container and the suite run; `depends`/`optdepends` corrected, the comment block rewritten. **Done 2026-09-26**, by reading the code and writing the consequence as a test (`kikid/tests/dep_audit.rs`, run by CI with each package removed): `gnome-keyring` → optdepends (saving a password says 1261 and nothing else changes; `libsecret` stays, it is the API); `git` → optdepends (a status that cannot run is "not a repository", `git.rs`; Cargo.lock has no git sources); `gvfs` → optdepends beside `gvfs-smb` (an SMB location answers "not supported"; `glib2` stays, the plugin links gio). No behaviour changed. | ½ day |
| L4 | **Checksums**: the release workflow writes each architecture's digest into `packaging/omarchy/PKGBUILD` as it tags; `SKIP` gone. **Done 2026-09-26**: a `checksums` job after `publish` fetches the published `.sha256` files and the tag's tarball, writes the three digests into both PKGBUILDs, regenerates `.SRCINFO`, builds the source package from the tag to prove the digest, and commits to main — only while main's `pkgver` is still that version. In the tree between a bump and its release the lines hold a digest that matches nothing (64 zeros), so a build from them fails on purpose; `SKIP` would have installed anything. | ½ day |
| L5 | **The AUR** — **not doing** (2026-09-26): closed to us; the local route needs nothing from it. `packaging/omarchy` is the store's PKGBUILD, the release keeps its checksums in step. | — |
| L6 | **The store PR**: fork `omarchy-pkgs`, `bin/add-package kiki` (or `--local --scaffold`), the minimal `.omarchy/package.json`, the repository's dry-run inspected, then a pull request with the testing statement of decision 5. **Prepared 2026-09-26**: `.omarchy/package.json` written (`local`, the GitHub release provider, `v{version}`); the fork, the scaffold run and the pull request are the owner's — "Owner's steps" below. | ½ day |

About **3 working days**, after `01-daemon-on-demand.md`. L1–L4 are worth doing whatever the
store decides: they are what make the package reproducible by anyone.

## Acceptance

- In a clean `archlinux:latest` container, with nothing but `packaging/PKGBUILD` copied in:
  `makepkg -s` fetches the tagged tarball, verifies its checksum and builds a package that
  installs and runs.
- `KIKI_LOCAL_SRC=1 makepkg` still builds the working tree, and CI still packages the commit
  under test.
- `namcap` output is in the CI log and every warning has an answer in the PKGBUILD's comments
  or in this plan. *(CI proves this; namcap is not on the owner's machine. The first run's
  output is still to be read.)*
- `desktop-file-validate` is silent; kiki appears once in the launcher.
- No `systemctl` in `kiki.install`; the post-install message is two lines or fewer and asks for
  nothing (`01-daemon-on-demand.md`).
- Removing `gnome-keyring`, `git` or `gvfs` in a container has a written, tested consequence.
  *(Written in the PKGBUILD; tested by CI's dependency audit, `kikid/tests/dep_audit.rs`.)*
- the store package's checksums are the release's own digests; no `SKIP` anywhere. *(From the first
  release after this lands; until then the lines are a digest that matches nothing.)*
- The pull request states what was tested on each architecture, and that aarch64 is not driven.
- `make lint`, `cargo test`, `make test-qml`, `tests/e2e/run.sh` green.

## Owner's steps

What only the owner can do, in order, once this has landed and a release has run the
`checksums` job at least once (so the PKGBUILDs carry real digests):

**L5, the AUR — not doing.** The AUR cannot be done from here; the local route stands on its
own and needs nothing from the AUR.

**L6, the store.** Fork `omarchy-pkgs`; `bin/add-package kiki --local --scaffold`; copy
`packaging/omarchy/PKGBUILD`, `kiki.install` and `.SRCINFO` into `pkgbuilds/kiki/`, and
`.omarchy/package.json` from this tree; run the repository's dry-run and read it; open the pull
request. Its text, per decision 5: what kiki is; that the package repackages the tagged GitHub
release's asset for each architecture — the bytes CI built and tested — with the digest the
release workflow writes from the asset it published, and that the source package
(`packaging/PKGBUILD`) builds the same release from its tarball in CI; that x86_64 is built,
unit-tested, packaged and driven through the whole e2e suite under `cage` on every release, and
that **aarch64 is built and unit-tested on a native ARM runner but has never been driven under a
compositor** — stated, not excused; and that CI runs `namcap` and a from-nothing `makepkg -s`
on every commit.

**`install.sh` is untouched** and keeps working: it fetches the release's `.pkg.tar.zst`, which
none of this changes. One thing to know: it decides "already installed" by comparing
`pacman -Q kiki` to `<version>-1`, so a store package with a
different `pkgrel` reads as not installed and is reinstalled at the same version — harmless, and
a two-line fix when there is a second package to compare against.

## What this plan does not do

Sign, publish, promote or approve anything; ask for a faster release ring; or treat the
checklist as permission. A maintainer decides whether kiki belongs in the store; if the answer
is "use the AUR", that is a door closed to us, and `install.sh` remains the way in.
