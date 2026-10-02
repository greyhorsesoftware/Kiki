# The compiler flags makepkg would build with, for a cargo run outside makepkg — so the two
# share one `target/` instead of each throwing the other's build away: cargo fingerprints every
# crate by RUSTFLAGS (and a C-building crate by CFLAGS and LDFLAGS), and makepkg exports the
# values in makepkg.conf and its .d. Read the way makepkg reads them; nothing appended, since
# the PKGBUILD's options are `!lto` and `!debug`, the two that would append. Sourced by the CI
# steps that test before `makepkg` (.github/workflows/ci.yml, docs/0.5.0/08-ci-once.md).
. /usr/share/makepkg/util/config.sh
load_makepkg_config
export CPPFLAGS CFLAGS CXXFLAGS LDFLAGS RUSTFLAGS
