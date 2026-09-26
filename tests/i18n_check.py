#!/usr/bin/env python3
"""The catalogs stay whole (docs/0.2.0/02-localization.md, L4): every key the QML asks
`T.tr` for exists in en.js and in every other catalog; no catalog holds a key nobody asks for;
the {placeholders} of a key are the same in every language. Run by `make lint`."""
import glob, json, re, sys

ROOT = "qml/kiki"


def load(path):
    keys = {}
    for m in re.finditer(r'^\s*"([^"]+)":\s*(.*?),?\s*$', open(path, encoding="utf-8").read(), re.M):
        key, value = m.group(1), m.group(2).rstrip(",")
        try:
            keys[key] = json.loads(value)
        except json.JSONDecodeError:
            keys[key] = json.loads(re.sub(r'(\{|,)\s*(one|other)\s*:', r'\1 "\2":', value))
    return keys


catalogs = {p.rsplit("/", 1)[1][:-3]: load(p) for p in sorted(glob.glob(f"{ROOT}/i18n/*.js"))}
en = catalogs.get("en")
if en is None:
    print("i18n: no en.js")
    sys.exit(1)

asked, prefixes = {}, set()
sources = sorted(glob.glob(f"{ROOT}/**/*.qml", recursive=True)) + sorted(glob.glob(f"{ROOT}/*.js"))
for path in sources:
    if "/i18n/" in path:
        continue
    for i, line in enumerate(open(path, encoding="utf-8"), 1):
        if not re.search(r"\b(tr|has)\(", line):
            continue
        # Every dotted key quoted on a line that asks `tr`: the first argument, or the arms of
        # a ternary choosing between keys (`T.tr(a ? "x.one" : "x.other", …)`).
        for m in re.finditer(r'"([a-z][A-Za-z0-9]*(?:\.[A-Za-z0-9]+)+)"', line):
            asked.setdefault(m.group(1), []).append(f"{path}:{i}")
        # A key built from a prefix — `T.tr("kind." + kind)` — asks for every key under it.
        for m in re.finditer(r'(?:tr|has)\(\s*"([a-z]+\.(?:[A-Za-z0-9]+\.)*)"\s*\+', line):
            prefixes.add(m.group(1))


def placeholders(v):
    if isinstance(v, dict):
        return set().union(*(placeholders(x) for x in v.values()))
    return set(re.findall(r"\{(\w+)\}", str(v)))


errors = []
for key, where in asked.items():
    if key not in en:
        errors.append(f"asked but not in en.js: {key}  ({where[0]})")
for lang, keys in catalogs.items():
    if lang == "en":
        continue
    for key in en:
        if key not in keys:
            errors.append(f"{lang}.js lacks: {key}")
        elif placeholders(keys[key]) != placeholders(en[key]):
            errors.append(f"{lang}.js placeholders differ for {key}: {sorted(placeholders(keys[key]))} vs en {sorted(placeholders(en[key]))}")
    for key in keys:
        if key not in en:
            errors.append(f"{lang}.js has a key en.js does not: {key}")
for key in en:
    if key not in asked and not any(key.startswith(p) for p in prefixes):
        errors.append(f"nobody asks for: {key}")
if errors:
    print("\n".join(errors))
    print(f"i18n: {len(errors)} problem(s)")
    sys.exit(1)
print(f"i18n: {len(en)} keys, {len(catalogs)} catalogs, all whole")
