#!/usr/bin/env bash
# Installs kiki on Arch / Omarchy from the latest GitHub release:
#
#   curl -fsSL https://raw.githubusercontent.com/greyhorsesoftware/Kiki/main/install.sh | bash
#
# It downloads the pacman package built by the release workflow for this machine's architecture,
# checks its sha256 against the one published beside it, and hands it to `pacman -U`, which
# resolves the dependencies from the official repositories. Run it again to upgrade.
#
#   KIKI_VERSION=v0.1.0 …   install that release rather than the latest
#   KIKI_PACKAGE=/path.pkg.tar.zst …   install a package already on disk (the release checklist)
set -euo pipefail

repo="greyhorsesoftware/Kiki"
api="https://api.github.com/repos/${repo}/releases"

say() { printf ':: %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

command -v pacman >/dev/null || die "kiki is packaged for Arch Linux (Omarchy); this system has no pacman."
command -v curl >/dev/null || die "curl is needed to fetch the release."

arch="$(uname -m)"
case "$arch" in
  x86_64|aarch64) ;;
  *) die "no kiki package is built for ${arch} (x86_64 and aarch64 are)." ;;
esac

as_root() { if [ "$(id -u)" -eq 0 ]; then "$@"; else sudo "$@"; fi; }

# `curl … | bash` makes the script pacman's stdin: its "Proceed? [Y/n]" read the end of the
# script, took that for a no, and the script said "done" over a package that was never
# installed. Give pacman the terminal when there is one, answer yes when there is none, and
# then look, rather than trust an exit code, before saying anything.
install_package() {
  if ( : </dev/tty ) 2>/dev/null; then
    as_root pacman -U --needed "$1" </dev/tty
  else
    as_root pacman -U --needed --noconfirm "$1"
  fi
  pacman -Q kiki >/dev/null 2>&1 || die "pacman did not install kiki (declined, or a dependency it could not resolve — see above)."
}

if [ -n "${KIKI_PACKAGE:-}" ]; then
  [ -f "$KIKI_PACKAGE" ] || die "${KIKI_PACKAGE}: no such file"
  say "installing ${KIKI_PACKAGE}"
  install_package "$KIKI_PACKAGE"
  say "done. kiki is in the launcher, and 'kiki' starts it."
  exit 0
fi

# Which release: the tag asked for, or the latest that is not a draft or a pre-release.
if [ -n "${KIKI_VERSION:-}" ]; then
  tag="${KIKI_VERSION}"; [ "${tag#v}" = "$tag" ] && tag="v${tag}"
  release_url="${api}/tags/${tag}"
else
  release_url="${api}/latest"
fi
json="$(curl -fsSL -H 'Accept: application/vnd.github+json' "$release_url")" || die "could not read ${release_url}"
tag="$(printf '%s' "$json" | sed -n 's/^  "tag_name": "\(.*\)",$/\1/p' | head -1)"
[ -n "$tag" ] || die "no release found at ${release_url}"
version="${tag#v}"
file="kiki-${version}-1-${arch}.pkg.tar.zst"
base="https://github.com/${repo}/releases/download/${tag}"

if pacman -Q kiki >/dev/null 2>&1 && [ "$(pacman -Q kiki | cut -d' ' -f2)" = "${version}-1" ]; then
  say "kiki ${version} is already installed."
  exit 0
fi

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
say "fetching kiki ${version} for ${arch}"
curl -fsSL -o "${tmp}/${file}" "${base}/${file}" || die "no package ${file} in release ${tag}"
curl -fsSL -o "${tmp}/${file}.sha256" "${base}/${file}.sha256" || die "no checksum for ${file} in release ${tag}"
# The checksum file was written where the package was built; only the hash matters here.
want="$(cut -d' ' -f1 "${tmp}/${file}.sha256")"
have="$(sha256sum "${tmp}/${file}" | cut -d' ' -f1)"
[ "$want" = "$have" ] || die "checksum mismatch for ${file}: expected ${want}, got ${have}"

say "installing (pacman will ask for your password and confirm the dependencies)"
install_package "${tmp}/${file}"
[ "$(pacman -Q kiki | cut -d' ' -f2)" = "${version}-1" ] || die "kiki is installed, but not ${version}: pacman kept what was there."
say "done. kiki ${version} is in the launcher, and 'kiki' starts it."
