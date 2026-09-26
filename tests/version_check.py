#!/usr/bin/env python3
"""One version everywhere: Cargo.toml's is the one, and the window (`qml/kiki/version.js`) and
the two PKGBUILDs must say the same — the socket is named for it and a window refuses a daemon
of another version, so the two halves of a build must agree on what they are. Run by `make lint`."""
import re, sys

cargo = re.search(r'^version = "([^"]+)"', open("Cargo.toml", encoding="utf-8").read(), re.M).group(1)
found = {
    "qml/kiki/version.js": re.search(r'var version = "([^"]+)"', open("qml/kiki/version.js", encoding="utf-8").read()).group(1),
    "packaging/PKGBUILD": re.search(r"^pkgver=(\S+)", open("packaging/PKGBUILD", encoding="utf-8").read(), re.M).group(1),
    "packaging/aur/kiki-bin/PKGBUILD": re.search(r"^pkgver=(\S+)", open("packaging/aur/kiki-bin/PKGBUILD", encoding="utf-8").read(), re.M).group(1),
}
wrong = {k: v for k, v in found.items() if v != cargo}
for k, v in wrong.items():
    print(f"version: {k} says {v}, Cargo.toml says {cargo}")
print(f"version: {cargo}" + ("" if not wrong else f", {len(wrong)} disagree"))
sys.exit(1 if wrong else 0)
