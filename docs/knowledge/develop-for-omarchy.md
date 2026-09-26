# Develop for Omarchy — Unofficial Packaging Checklist

> A portable checklist for contributors and LLM-assisted package reviews.
> This is an independent community resource, not official Omarchy guidance.
> Always verify current requirements in the upstream repositories before acting.

Last reviewed: 2026-09-26

## How to use this with an LLM

Add this Markdown file to your LLM project, knowledge base, or skill resources.
Ask the LLM to review a proposed Omarchy application package against every
check below, mark each item as pass, fail, or not verified, cite the file or
command that supports its conclusion, and never invent missing evidence.

An LLM using this checklist should:

- Treat the official Omarchy package repository and current Arch Linux documentation as authoritative.
- Distinguish observed facts from assumptions and clearly identify anything that was not tested.
- Never request, expose, or use signing keys, API keys, credentials, private infrastructure, or production access.
- Avoid publishing or installing anything unless the user explicitly asks and the action is within scope.
- Preserve upstream copyright notices, licences, trademarks, and attribution.
- Recommend maintainer review when repository policy or security impact is unclear.

## 1. Choose the right source

- [ ] Confirm the application has a stable upstream release, a real project homepage, and a licence that permits redistribution.
- [ ] Search the official Omarchy repository and the AUR first so you do not create a duplicate package.
- [ ] Prefer the AUR route when a healthy package already exists there; use a local Omarchy package when custom packaging is genuinely needed.
- [ ] Verify every release URL, asset name, architecture, version tag, and checksum against the upstream release.

## 2. Scaffold the Omarchy package

Work in a fork of the official package repository. Each package belongs in
`pkgbuilds/<package-name>/` and carries Omarchy metadata in
`.omarchy/package.json`.

```sh
# Existing AUR package
bin/add-package package-name

# New package maintained in this repository
bin/add-package package-name --local --scaffold
```

- [ ] Use a lowercase, descriptive package name consistent with Arch naming conventions.
- [ ] Keep `.omarchy/package.json` minimal: declare whether the source is `aur` or `local`, then add only behavior the package needs.
- [ ] If an AUR package needs lasting changes, store focused patches under `.omarchy/patches/` rather than editing synced files without a reproducible patch.
- [ ] Use a custom upstream hook only when the repository's declarative GitHub release provider cannot describe the vendor's releases.

## 3. Write trustworthy package metadata

- [ ] Set accurate `pkgver`, `pkgrel`, description, architecture, URL, and licence fields.
- [ ] List runtime dependencies in `depends` and build-only tools in `makedepends`; do not rely on software that merely happens to be installed on your machine.
- [ ] Use `provides`, `conflicts`, and `replaces` only when their exact package-management semantics are required.
- [ ] Use cryptographic checksums for downloaded release assets. Never copy a checksum from an untrusted mirror or disable verification just to make a build pass.
- [ ] Map every supported architecture to the correct asset, or explicitly limit `arch` when upstream does not support another architecture.

## 4. Package files safely

- [ ] Install bundled applications under an appropriate system location such as `/opt/<app>`, with launchers in standard executable paths.
- [ ] Keep PKGBUILDs, metadata, desktop entries, and data files non-executable; grant `755` only to actual programs and scripts.
- [ ] Avoid setuid, broad write permissions, unnecessary capabilities, and elevated post-install actions. Document any privilege that is truly unavoidable.
- [ ] Do not hard-code a username, home directory, local build path, private host, credential, API key, or maintainer-only infrastructure.
- [ ] Make uninstalling remove packaged files without deleting user-created data or configuration.

## 5. Integrate with the desktop

- [ ] Provide a valid `.desktop` launcher with matching executable, icon, categories, and startup behavior.
- [ ] Install icons into standard system icon locations and verify they appear in the Omarchy application launcher.
- [ ] Launch the app from both a terminal and the application menu under Hyprland; watch for Wayland, portal, tray, notification, and file-picker problems.
- [ ] Keep install messages short and limited to user actions that are genuinely required after installation.

## 6. Test like a new user

- [ ] Build from a clean checkout and, when practical, in a clean Arch chroot so undeclared local dependencies cannot hide mistakes.
- [ ] Run `namcap` against the PKGBUILD and built package, then review warnings rather than dismissing them automatically.
- [ ] Use the repository's dry-run option to inspect the build plan before running a real package build.
- [ ] Install the resulting package, exercise its main workflows as a normal user, reboot or log in again if integration depends on a new session, then uninstall it.
- [ ] Inspect the final package file list, ownership, permissions, dependencies, installed size, and absence of build-machine paths or secrets.
- [ ] Keep the download small. Prefer upstream release binaries over bundled toolchains, strip debug symbols, and split large optional assets into their own package; the store shows download and installed size on every card, and a multi-hundred-megabyte package is the first thing reviewers question.
- [ ] If multiple architectures are declared, test each one or clearly state which architecture remains unverified.

## 7. Prepare a useful pull request

- [ ] Keep the change focused: package files, required Omarchy metadata, and only the patches or hooks needed to reproduce the result.
- [ ] Explain the app, why it belongs in Omarchy, where the source came from, how updates are tracked, and exactly what you tested.
- [ ] Include important limitations, architecture gaps, unusual permissions, proprietary licensing, network services, or accounts the app requires.
- [ ] Review the complete diff for generated junk, personal information, credentials, local paths, and unrelated edits before pushing.
- [ ] Expect maintainer revisions. A strong submission is safe, reproducible, understandable, and easy to refine.

## Extra tips that prevent common failures

### Pin inputs together

When a URL, version, asset name, or checksum changes, review them as one unit.
A valid checksum for the wrong artifact is still the wrong package.

### Do not publish from contributor machines

Build, test, and open a pull request. Repository signing keys, production
syncing, release promotion, and infrastructure secrets belong to maintainers.

### Prefer reproducible customization

For AUR-derived packages, use the repository's patch and package-worktree
workflow so the next upstream sync can recreate your changes.

### Treat fast releases carefully

Release rings and channel restrictions affect how packages reach users. Do not
opt into faster delivery unless maintainers request it and the risk is understood.

### Consider release quarantine

For automated upstream tracking, a minimum release age can give compromised or
broken releases time to be noticed before packaging.

### Test the actual desktop path

A binary launching in a terminal does not prove its desktop entry, icon, portal
integration, environment, or Wayland behavior works from the launcher.

## Authoritative references

- [Official Omarchy package repository](https://github.com/omacom/omarchy-pkgs)
- [ArchWiki: Creating packages](https://wiki.archlinux.org/title/Creating_packages)
- [ArchWiki: Building in a clean chroot](https://wiki.archlinux.org/title/DeveloperWiki:Building_in_a_clean_chroot)

## Important boundary

This checklist helps contributors prepare and review work. It does not grant
authority to sign, publish, promote, or approve packages, and it does not
replace current upstream contribution rules or maintainer decisions.
