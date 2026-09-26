.pragma library

// The window's own version, the same string as Cargo.toml's (`make lint` checks the two agree):
// the daemon's socket is named for it, and a daemon of another version is refused on Hello —
// a window and a daemon are a pair (docs/0.2.0/03-fixes.md, 2026-09-25).
var version = "0.2.2"
