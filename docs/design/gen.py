import os
OUT = "/Users/david/Documents/GitHub/kiki/docs/design"

# ---------- Tokyo Night palette ----------
BG="#1a1b26"; BGD="#16161e"; HL="#292e42"; LINE="#292e42"; GUT="#3b4261"
FG="#c0caf5"; FGD="#a9b1d6"; CM="#565f89"
BLUE="#7aa2f7"; CYAN="#7dcfff"; PURPLE="#bb9af7"; GREEN="#9ece6a"; YELLOW="#e0af68"; RED="#f7768e"

# ---------- icons (16px grid, stroke) ----------
ALIASES = {"phone": "hdd", "mail": "doc", "message": "doc", "share": "arr-u", "sparkle": "info", "eject": "arr-u", "usb": "hdd"}
def ico(name, size=16, color="currentColor", sw=1.5):
    paths = {
        "gear": '<g transform="scale(0.6667)"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path></g>',
        "folder": '<path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v6.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12.5z"></path>',
        "file": '<path d="M4 2h5l3 3v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z"></path><path d="M9 2v3h3"></path>',
        "doc": '<path d="M4 2h5l3 3v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V3a1 1 0 0 1 1-1z"></path><path d="M9 2v3h3"></path><path d="M5.5 8.5h5"></path><path d="M5.5 11h5"></path>',
        "image": '<rect x="2" y="3" width="12" height="10" rx="1"></rect><circle cx="5.5" cy="6.5" r="1"></circle><path d="m14 11-3.5-3.5L5 13"></path>',
        "code": '<path d="m5 5-3 3 3 3"></path><path d="m11 5 3 3-3 3"></path><path d="m9.5 3-3 10"></path>',
        "archive": '<rect x="2" y="3" width="12" height="10" rx="1"></rect><path d="M2 6.5h12"></path><path d="M6.5 9.5h3"></path>',
        "home": '<path d="M2.5 7.5 8 3l5.5 4.5V13a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1z"></path><path d="M6.5 14V9.5h3V14"></path>',
        "download": '<path d="M8 2v8"></path><path d="m5 7 3 3 3-3"></path><path d="M3 12v1.5h10V12"></path>',
        "trash": '<path d="M3 4h10"></path><path d="M6 4V2.5h4V4"></path><path d="m4 4 .6 9.5h6.8L12 4"></path>',
        "hdd": '<rect x="2" y="9" width="12" height="4" rx="1"></rect><path d="M2.5 9 4 3.5h8L13.5 9"></path><path d="M11 11h1"></path>',
        "server": '<rect x="2" y="2.5" width="12" height="4" rx="1"></rect><rect x="2" y="9.5" width="12" height="4" rx="1"></rect><path d="M4.5 4.5h1"></path><path d="M4.5 11.5h1"></path>',
        "cloud": '<path d="M5 13a3 3 0 0 1-.5-5.96A4 4 0 0 1 12.3 8.5 2.5 2.5 0 0 1 12 13z"></path>',
        "search": '<circle cx="7" cy="7" r="4"></circle><path d="m10 10 3.5 3.5"></path>',
        "chev-r": '<path d="m6 3 5 5-5 5"></path>',
        "chev-d": '<path d="m3 6 5 5 5-5"></path>',
        "arr-l": '<path d="M13 8H3"></path><path d="m7 4-4 4 4 4"></path>',
        "arr-r": '<path d="M3 8h10"></path><path d="m9 4 4 4-4 4"></path>',
        "grid": '<rect x="2.5" y="2.5" width="4.5" height="4.5"></rect><rect x="9" y="2.5" width="4.5" height="4.5"></rect><rect x="2.5" y="9" width="4.5" height="4.5"></rect><rect x="9" y="9" width="4.5" height="4.5"></rect>',
        "list": '<path d="M3 4h10"></path><path d="M3 8h10"></path><path d="M3 12h10"></path>',
        "columns": '<rect x="2" y="2.5" width="12" height="11" rx="1"></rect><path d="M6 2.5v11"></path><path d="M10 2.5v11"></path>',
        "sidebar": '<rect x="2" y="2.5" width="12" height="11" rx="1"></rect><path d="M6 2.5v11"></path>',
        "music": '<path d="M6 12.5V3.5l7-1.5v9"></path><ellipse cx="4" cy="12.5" rx="2" ry="1.8"></ellipse><ellipse cx="11" cy="11" rx="2" ry="1.8"></ellipse>',
        "video": '<rect x="2" y="3.5" width="12" height="9" rx="1"></rect><path d="m6.5 6.3 4 2.7-4 2.7z"></path>',
        "plus": '<path d="M8 3v10"></path><path d="M3 8h10"></path>',
        "x": '<path d="m4 4 8 8"></path><path d="m12 4-8 8"></path>',
        "key": '<circle cx="5.5" cy="8" r="3"></circle><path d="M8.5 8H14"></path><path d="M12 8v2.5"></path>',
        "split": '<rect x="2" y="2.5" width="5" height="11" rx="1"></rect><rect x="9" y="2.5" width="5" height="11" rx="1"></rect>',
        "info": '<circle cx="8" cy="8" r="6"></circle><path d="M8 7.5v3.5"></path><path d="M8 5.2v.3"></path>',
        "mirror": '<path d="M2 8h4"></path><path d="m4 5.5-2.5 2.5L4 10.5"></path><path d="M10 8h4"></path><path d="m12 5.5 2.5 2.5-2.5 2.5"></path><path d="M8 2v12" stroke-dasharray="1.6 1.6"></path>',
        "arr-u": '<path d="M8 13V3"></path><path d="m4 7 4-4 4 4"></path>',
        "arr-dn": '<path d="M8 3v10"></path><path d="m4 9 4 4 4-4"></path>',
        "equals": '<path d="M3 6h10"></path><path d="M3 10h10"></path>',
        "check": '<path d="m3 8.5 3.5 3.5L13 4.5"></path>',
        "warn": '<path d="M8 2.5 14 13H2z"></path><path d="M8 6.5v3"></path><path d="M8 11.3v.2"></path>',
        "sort-up": '<path d="m4 9 4-4 4 4"></path>',
    }
    if name not in paths:
        name = ALIASES.get(name, name)
    return (f'<svg width="{size}" height="{size}" viewBox="0 0 16 16" fill="none" stroke="{color}" '
            f'stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round">{paths[name]}</svg>')

HELMET = f"""<helmet>
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Cascadia+Mono:wght@400;600&amp;display=swap">
  <style>
    body {{ margin: 0; background: {BGD}; font-family: "Cascadia Mono", "SF Mono", Menlo, Consolas, monospace; font-size: 13px; line-height: 1.4; color: {FG}; -webkit-font-smoothing: antialiased; }}
    a {{ color: {BLUE}; }} a:hover {{ color: {CYAN}; }}
    svg {{ display: block; flex: none; }}
  </style>
</helmet>"""

def doc(body, props=None):
    script = ""
    if props:
        script = f"\n<script data-dc-script data-props='{props}'>\nclass Component extends DCLogic {{}}\n</script>"
    return f"""<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <script src="./support.js"></script>
</head>
<body>
<x-dc>
{HELMET}
{body}
</x-dc>{script}
</body>
</html>
"""

# ---------- sidebar ----------
def side_item(icon, label, color, active=False, muted=False):
    bg = HL if active else "transparent"
    fg = FG if active else FGD
    return (f'<div style="display: flex; align-items: center; gap: 10px; height: 30px; padding: 0 16px; margin: 0 8px; '
            f'background: {bg}; color: {fg}; border-radius: 2px;">{ico(icon, color=color)}'
            f'<span style="flex-grow: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{label}</span></div>')

def side_header(label, plus=False):
    btn = ""
    if plus:
        btn = (f'<div style="display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; '
               f'color: {CM}; border-radius: 2px;" title="Add a location (SFTP, FTPS)">{ico("plus", size=14)}</div>')
    return (f'<div style="display: flex; align-items: center; justify-content: space-between; height: 28px; padding: 0 16px 0 24px; '
            f'margin-top: 12px; font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: {CM};">'
            f'<span>{label}</span>{btn}</div>')

def sidebar(active="home"):
    return f"""<div style="display: flex; flex-direction: column; width: 224px; flex: none; background: {BGD}; border-right: 1px solid {LINE}; padding: 12px 0; box-sizing: border-box; overflow: hidden;">
  {side_header("Favorites")}
  <div style="display: flex; flex-direction: column; gap: 1px;">
    {side_item("home", "Home", BLUE, active == "home")}
    {side_item("folder", "Desktop", BLUE)}
    {side_item("folder", "Documents", BLUE)}
    {side_item("download", "Downloads", BLUE)}
    {side_item("folder", "Pictures", BLUE)}
    {side_item("folder", "Projects", BLUE, active == "projects")}
    {side_item("trash", "Trash", FGD, active == "trash")}
  </div>
  {side_header("Locations", plus=True)}
  <div style="display: flex; flex-direction: column; gap: 1px;">
    {side_item("hdd", "System", FGD)}
    {side_item("hdd", "SanDisk 64G", FGD)}
    {side_item("server", "homelab · sftp", GREEN, active == "homelab")}
    {side_item("server", "nas · ftps", CYAN)}
  </div>
  <div style="flex-grow: 1;"></div>
  <div style="display: flex; align-items: center; gap: 10px; padding: 0 24px; font-size: 11px; color: {CM};">
    <div style="flex-grow: 1; height: 4px; background: {HL}; border-radius: 2px; overflow: hidden;"><div style="width: 58%; height: 100%; background: {GUT};"></div></div>
    <span>412 GB free</span>
  </div>
</div>"""

# ---------- toolbar ----------
def nav_btn(icon, enabled=True):
    c = FGD if enabled else GUT
    return (f'<div style="display: flex; align-items: center; justify-content: center; width: 28px; height: 28px; '
            f'color: {c}; border-radius: 2px;">{ico(icon)}</div>')

def crumb(label, last=False):
    c = FG if last else CM
    w = "600" if last else "400"
    return f'<span style="color: {c}; font-weight: {w};">{label}</span>'


def search_box(placeholder, search):
    if not search:
        return (f'<div style="display: flex; align-items: center; gap: 8px; width: 260px; height: 30px; padding: 0 10px; flex: none; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; color: {CM}; box-sizing: border-box;">{ico("search", size=14)}'
                f'<span style="flex-grow: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{placeholder}</span><span style="font-size: 10px; padding: 1px 5px; border: 1px solid {GUT}; border-radius: 2px; color: {GUT};">/</span></div>')
    text, scope = search
    chip = ""
    if scope != "folder":
        label = "Everywhere" if scope == "everywhere" else scope
        chip = (f'<span style="display: flex; align-items: center; gap: 4px; height: 20px; padding: 0 6px; margin-right: 2px; background: {HL}; color: {BLUE}; '
                f'font-size: 11px; font-weight: 600; border-radius: 2px; white-space: nowrap;">{label}<span style="color: {CM}; font-weight: 400;">:</span></span>')
    scope_btn = chip if chip else f'<span style="display: flex; align-items: center; height: 20px; padding: 0 4px; margin-right: 2px; color: {CM};">{ico("chev-d", size=10)}</span>'
    return (f'<div style="display: flex; align-items: center; gap: 6px; width: 320px; height: 30px; padding: 0 8px 0 10px; flex: none; background: {BGD}; border: 1px solid {BLUE}; border-radius: 2px; color: {FG}; box-sizing: border-box;">{ico("search", size=14, color=BLUE)}'
            f'{scope_btn}<span style="flex-grow: 1; white-space: nowrap;">{text}<span style="display: inline-block; width: 1px; height: 14px; margin-left: 1px; background: {FG}; vertical-align: -2px;"></span></span>'
            f'<span style="display: flex; align-items: center; justify-content: center; width: 16px; height: 16px; color: {CM};">{ico("x", size=10)}</span></div>')

def toolbar(crumbs, view, search_placeholder, split=False, inspector=False, mirror=None, search=None, merged=True):
    parts = []
    for i, c in enumerate(crumbs):
        if i: parts.append(f'<span style="color: {GUT};">{ico("chev-r", size=12)}</span>')
        parts.append(crumb(c, last=(i == len(crumbs) - 1)))
    crumb_html = "".join(parts)
    def seg(icon, key):
        on = key == view
        bg = HL if on else "transparent"; c = BLUE if on else CM
        return (f'<div style="display: flex; align-items: center; justify-content: center; width: 34px; height: 28px; '
                f'background: {bg}; color: {c}; border-radius: 2px;">{ico(icon)}</div>')
    return f"""<div style="display: flex; align-items: center; gap: 8px; height: 48px; flex: none; padding: 0 12px; border-bottom: 1px solid {LINE}; box-sizing: border-box;">
  <div style="display: flex; gap: 2px;">{nav_btn("arr-l")}{nav_btn("arr-r", enabled=False)}</div>
  <div style="display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 10px; margin-left: 4px; flex-grow: 1; min-width: 0; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; white-space: nowrap; overflow: hidden;">{crumb_html}</div>
  {search_box(search_placeholder, search)}
  <div style="display: flex; align-items: center; justify-content: center; gap: 4px; width: 52px; height: 34px; flex: none; background: {HL if merged else BGD}; border: 1px solid {BLUE if merged else LINE}; border-radius: 2px; box-sizing: border-box; color: {BLUE};" title="View: icon, list, columns, mirror; show hidden files">{ico({"icon": "grid", "list": "list", "columns": "columns", "mirror": "mirror"}[view])}{ico("chev-d", size=10, color=CM)}</div>
  {"" if merged else f'<div style="display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex: none; background: {HL if split else BGD}; border: 1px solid {BLUE if split else LINE}; border-radius: 2px; color: {BLUE if split else CM}; box-sizing: border-box;" title="Split: local and remote side by side">{ico("split")}</div>'}
  <div style="display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex: none; background: {HL if inspector else BGD}; border: 1px solid {BLUE if inspector else LINE}; border-radius: 2px; color: {BLUE if inspector else CM}; box-sizing: border-box;" title="Toggle inspector panel">{ico("info")}</div>
  {"" if merged else mirror_btn(mirror)}
  <div style="display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex: none; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; color: {CM}; box-sizing: border-box;" title="Settings (Ctrl+,)">{ico("gear")}</div>
</div>"""

def mirror_btn(state):
    if state is None: return ""
    on = bool(state)
    return (f'<div style="display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex: none; background: {HL if on else BGD}; '
            f'border: 1px solid {BLUE if on else LINE}; border-radius: 2px; color: {BLUE if on else CM}; box-sizing: border-box;" title="Mirror local and remote">{ico("mirror")}</div>')

def key(k, label):
    return (f'<div style="display: flex; align-items: center; gap: 5px; white-space: nowrap;">'
            f'<span style="padding: 1px 5px; border: 1px solid {GUT}; border-radius: 2px; font-size: 10px; color: {FGD};">{k}</span>'
            f'<span style="color: {CM};">{label}</span></div>')

BROWSE_KEYS = [("Enter","open"),("F2","rename"),("Del","trash"),("^C","copy"),("^V","paste"),("/","search"),("^Z","undo"),("?","all keys")]
COLUMNS_KEYS = [("Enter","open"),("h l","columns"),("F2","rename"),("Del","trash"),("^C","copy"),("^V","paste"),("^Z","undo"),("?","all keys")]
SPLIT_KEYS = [("Tab","switch pane"),("^C ^V","transfer"),("^M","mirror…"),("^1-4","view"),("?","all keys")]

def statusbar(text, keys=BROWSE_KEYS):
    chips = "".join(key(k, l) for k, l in keys)
    return (f'<div style="display: flex; align-items: center; gap: 14px; height: 28px; flex: none; padding: 0 14px; '
            f'border-top: 1px solid {LINE}; font-size: 11px; color: {CM}; box-sizing: border-box; overflow: hidden;">'
            f'<div style="display: flex; align-items: center; gap: 14px; overflow: hidden;">{chips}</div><span style="flex-grow: 1;"></span><span style="white-space: nowrap;">{text}</span></div>')

def window(sidebar_html, main_html):
    return (f'<div style="display: flex; width: 1200px; height: 760px; background: {BG}; border: 2px solid {BLUE}; box-sizing: border-box; overflow: hidden;">'
            f'{sidebar_html}<div style="position: relative; display: flex; flex-direction: column; flex-grow: 1; min-width: 0;">{main_html}</div></div>')

# ---------- sample data ----------
KIND = {"folder": ("folder", BLUE), "md": ("doc", CYAN), "sh": ("code", GREEN), "rs": ("code", GREEN), "toml": ("code", FGD),
        "png": ("image", PURPLE), "jpg": ("image", PURPLE), "pdf": ("doc", RED), "zip": ("archive", YELLOW), "txt": ("doc", FGD), "lock": ("file", CM)}

HOME = [("Desktop","folder","11 Sep 2026 09:14","—"),("Documents","folder","8 Sep 2026 17:40","—"),("Downloads","folder","12 Sep 2026 13:58","—"),
        ("Music","folder","2 Aug 2026 20:11","—"),("Pictures","folder","10 Sep 2026 22:03","—"),("Projects","folder","12 Sep 2026 14:02","—"),
        ("Videos","folder","19 Jul 2026 16:30","—"),("install.sh","sh","3 Sep 2026 08:20","2.4 KB"),("notes.md","md","11 Sep 2026 23:47","6.1 KB"),
        ("omarchy-cheatsheet.pdf","pdf","30 Aug 2026 12:05","318 KB"),("screenshot-2026-09-12.png","png","12 Sep 2026 13:58","1.2 MB"),
        ("wallpapers.zip","zip","21 Aug 2026 19:22","84.5 MB")]
PROJECTS = [("dotfiles","folder"),("kiki","folder"),("omarchy","folder"),("waybar-themes","folder"),("TODO.md","md")]
KIKI = [("design","folder"),("src","folder"),("Cargo.lock","lock"),("Cargo.toml","toml"),("PLAN.md","md"),("README.md","md")]

def row(name, kind, selected=None, chevron=False):
    icon, color = KIND[kind]
    if selected == "active":
        bg, fg, ic, ch = BLUE, BG, BG, BG
    elif selected == "inactive":
        bg, fg, ic, ch = HL, FG, color, CM
    else:
        bg, fg, ic, ch = "transparent", FGD, color, GUT
    chev = f'<span style="color: {ch};">{ico("chev-r", size=12)}</span>' if (chevron and kind == "folder") else ""
    return (f'<div style="display: flex; align-items: center; gap: 8px; height: 28px; padding: 0 10px; background: {bg}; color: {fg};">'
            f'{ico(icon, color=ic)}<span style="flex-grow: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{name}</span>{chev}</div>')

# ---------- Main: columns view + inspector ----------
def column(items, selected_name, selected_mode):
    rows = "".join(row(n, k, selected_mode if n == selected_name else None, chevron=True) for n, k in items)
    return (f'<div style="display: flex; flex-direction: column; width: 220px; flex: none; padding: 6px 0; border-right: 1px solid {LINE}; '
            f'box-sizing: border-box; overflow: hidden;">{rows}</div>')

def checkbox(on):
    if on:
        return (f'<div style="display: flex; align-items: center; justify-content: center; width: 16px; height: 16px; background: {BLUE}; border: 1px solid {BLUE}; border-radius: 2px; box-sizing: border-box;">'
                f'<svg width="10" height="10" viewBox="0 0 16 16" fill="none" stroke="{BG}" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m3 8.5 3.5 3.5L13 4.5"></path></svg></div>')
    return f'<div style="width: 16px; height: 16px; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; box-sizing: border-box;"></div>'

def field(k, v, color=FG):
    return (f'<div style="display: flex; gap: 12px; font-size: 12px;"><span style="width: 88px; flex: none; color: {CM};">{k}</span>'
            f'<span style="flex-grow: 1; min-width: 0; color: {color}; overflow-wrap: anywhere;">{v}</span></div>')

def inspector_header(tab, it):
    def t(label, on):
        c = FG if on else CM; b = BLUE if on else "transparent"
        return f'<div style="display: flex; align-items: center; height: 32px; padding: 0 2px; margin-bottom: -1px; color: {c}; border-bottom: 2px solid {b};">{label}</div>'
    return f"""<div style="display: flex; align-items: flex-start; gap: 12px;">
    {ico(it["icon"], size=40, color=it["color"], sw=1)}
    <div style="display: flex; flex-direction: column; gap: 4px; min-width: 0; flex-grow: 1;">
      <div style="font-size: 15px; font-weight: 600; color: {FG}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{it["name"]}</div>
      <div style="display: flex; align-items: center; height: 24px; padding: 0 8px; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; font-size: 11px; color: {FGD}; overflow: hidden; white-space: nowrap;">{it["path"]}</div>
    </div>
  </div>
  <div style="display: flex; gap: 20px; border-bottom: 1px solid {LINE};">{t("General", tab == "general")}{t("Permissions", tab == "permissions")}</div>"""

def inspector_general(it):
    pv = "".join(f'<div style="white-space: pre;">{l}</div>' for l in it["preview"])
    return f"""<div style="display: flex; flex-direction: column; height: 150px; padding: 10px 12px; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; font-size: 11px; line-height: 1.5; color: {FGD}; box-sizing: border-box; overflow: hidden;">{pv}</div>
  <div style="display: flex; flex-direction: column; gap: 8px;">
    {field("Type", it["type"])}
    {field("Host", "local")}
    {field("Location", it["where"])}
  </div>
  <div style="display: flex; flex-direction: column; gap: 8px; padding-top: 12px; border-top: 1px solid {LINE};">
    {field("Size", it["size"])}
    {field("Modified", it["modified"])}
    {field("Created", it["created"])}
    {field("Owner", "david")}
    {field("Group", "david")}
    {it["extra"]}
  </div>
  <div style="flex-grow: 1;"></div>
  <div style="display: flex; gap: 8px;">
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; background: {BLUE}; color: {BG}; font-weight: 600; border-radius: 2px;">Open</div>
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; border: 1px solid {GUT}; color: {FGD}; border-radius: 2px;">Open with…</div>
  </div>"""

def inspector_permissions():
    def prow(label, r, w, x):
        return (f'<div style="display: grid; grid-template-columns: 88px 56px 56px 56px; align-items: center; height: 28px; font-size: 12px;">'
                f'<span style="color: {CM};">{label}</span><div style="display: flex; justify-content: center;">{checkbox(r)}</div>'
                f'<div style="display: flex; justify-content: center;">{checkbox(w)}</div><div style="display: flex; justify-content: center;">{checkbox(x)}</div></div>')
    head = (f'<div style="display: grid; grid-template-columns: 88px 56px 56px 56px; align-items: center; height: 24px; font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; color: {CM};">'
            f'<span></span><span style="text-align: center;">Read</span><span style="text-align: center;">Write</span><span style="text-align: center;">Exec</span></div>')
    return f"""<div style="display: flex; flex-direction: column; gap: 2px;">
    {head}
    {prow("Owner", True, True, False)}
    {prow("Group", True, False, False)}
    {prow("World", True, False, False)}
  </div>
  <div style="display: flex; flex-direction: column; gap: 8px; padding-top: 12px; border-top: 1px solid {LINE};">
    {field("Octal", "644")}
    {field("Symbolic", "-rw-r--r--")}
    {field("Owner", "david")}
    {field("Group", "david")}
  </div>
  <div style="display: flex; align-items: center; gap: 8px; font-size: 12px; color: {CM};">{checkbox(False)}<span>Apply to contained items</span></div>
  <div style="flex-grow: 1;"></div>
  <div style="display: flex; gap: 8px;">
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; background: {BLUE}; color: {BG}; font-weight: 600; border-radius: 2px;">Apply</div>
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; border: 1px solid {GUT}; color: {FGD}; border-radius: 2px;">Revert</div>
  </div>"""


README = {"name": "README.md", "path": "~/Projects/kiki/README.md", "icon": "doc", "color": CYAN, "type": "Markdown document", "where": "~/Projects/kiki",
          "size": "1.1 KB (1,126 bytes)", "modified": "12 Sep 2026 14:02", "created": "12 Sep 2026 11:36", "extra": field("Git", "modified, unstaged", YELLOW),
          "preview": ["# kiki", "", "A file manager for Omarchy.", "", "- Favorites and Locations", "- SFTP and FTPS locations", "- Icon, list and column views", "", "## Build", "", "    cargo run"]}
WALLPAPERS = {"name": "wallpapers.zip", "path": "~/wallpapers.zip", "icon": "archive", "color": YELLOW, "type": "Zip archive", "where": "~",
          "size": "84.5 MB (88,604,672 bytes)", "modified": "21 Aug 2026 19:22", "created": "21 Aug 2026 19:22", "extra": field("Contents", "38 files, 1 folder"),
          "preview": ["wallpapers/", "  tokyo-night-01.jpg", "  tokyo-night-02.jpg", "  kanagawa-01.jpg", "  nord-01.jpg", "  everforest-01.jpg", "  gruvbox-01.jpg", "  rose-pine-01.jpg", "  …"]}
PROJECTS_IT = {"name": "Projects", "path": "~/Projects", "icon": "folder", "color": BLUE, "type": "Folder", "where": "~",
          "size": "2.3 GB · 4 folders, 1 file", "modified": "12 Sep 2026 14:02", "created": "3 Jan 2026 10:12", "extra": field("Git", "4 repositories", GREEN),
          "preview": ["dotfiles/", "kiki/", "omarchy/", "waybar-themes/", "TODO.md"]}

def inspector(tab="general", standalone=False, it=None, side=False):
    it = it or README
    body = inspector_general(it) if tab == "general" else inspector_permissions()
    size = "width: 316px; height: 680px; background: " + BG + "; border: 2px solid " + BLUE + ";" if standalone else ("width: 300px; flex: none; border-left: 1px solid " + LINE + ";" if side else "flex-grow: 1; min-width: 0;")
    return f"""<div style="display: flex; flex-direction: column; gap: 16px; {size} padding: 16px 20px; box-sizing: border-box; overflow: hidden;">
  {inspector_header(tab, it)}
  {body}
</div>"""

INSPECTOR_PERMS = inspector("permissions", standalone=True)

main_cols = f"""<div style="display: flex; flex-grow: 1; min-height: 0; overflow: hidden;">
  {column([(n,k) for n,k,_,_ in HOME], "Projects", "inactive")}
  {column(PROJECTS, "kiki", "inactive")}
  {column(KIKI, "README.md", "active")}
  {inspector()}
</div>"""
MAIN = window(sidebar("home"), toolbar(["~","Projects","kiki"], "columns", "Search kiki") + main_cols + statusbar("6 items · 1 selected", keys=COLUMNS_KEYS))

# ---------- Icon view ----------
def tile(name, kind, selected=False):
    icon, color = KIND[kind]
    bg = "rgba(122, 162, 247, 0.16)" if selected else "transparent"
    border = BLUE if selected else "transparent"
    fg = FG if selected else FGD
    return (f'<div style="display: flex; flex-direction: column; align-items: center; gap: 8px; padding: 14px 6px 10px; '
            f'background: {bg}; border: 1px solid {border}; border-radius: 2px; color: {fg};">{ico(icon, size=44, color=color, sw=1)}'
            f'<span style="font-size: 12px; line-height: 1.3; text-align: center; max-width: 100%; overflow: hidden; text-overflow: ellipsis; '
            f'display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow-wrap: anywhere;">{name}</span></div>')

grid = ("".join(tile(n, k, selected=(n == "wallpapers.zip")) for n, k, _, _ in HOME))
icon_body = f"""<div style="flex-grow: 1; min-height: 0; padding: 18px; overflow: hidden;">
  <div style="display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 10px;">{grid}</div>
</div>"""
ICON = window(sidebar("home"), toolbar(["~"], "icon", "Search Home") + icon_body + statusbar("12 items · 1 selected"))

# ---------- List view ----------
def lrow(name, kind, mod, size, selected=False):
    icon, color = KIND[kind]
    kinds = {"folder":"Folder","sh":"Shell script","md":"Markdown","pdf":"PDF document","png":"PNG image","zip":"Zip archive"}
    bg = BLUE if selected else "transparent"; fg = BG if selected else FGD; ic = BG if selected else color; dim = BG if selected else CM
    return (f'<div style="display: grid; grid-template-columns: minmax(0, 1fr) 160px 80px 120px; align-items: center; gap: 12px; height: 28px; padding: 0 12px; background: {bg}; color: {fg};">'
            f'<div style="display: flex; align-items: center; gap: 8px; min-width: 0;">{ico(icon, color=ic)}<span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{name}</span></div>'
            f'<span style="color: {dim};">{mod}</span><span style="color: {dim}; text-align: right;">{size}</span><span style="color: {dim};">{kinds[kind]}</span></div>')

lhead = (f'<div style="display: grid; grid-template-columns: minmax(0, 1fr) 160px 80px 120px; align-items: center; gap: 12px; height: 30px; padding: 0 12px; '
         f'border-bottom: 1px solid {LINE}; font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: {CM};">'
         f'<div style="display: flex; align-items: center; gap: 6px; color: {FGD};"><span>Name</span>{ico("sort-up", size=12)}</div><span>Modified</span><span style="text-align: right;">Size</span><span>Kind</span></div>')
lrows = "".join(lrow(n, k, m, sz, selected=(n == "Projects")) for n, k, m, sz in HOME if n != "wallpapers.zip")
list_body = f'<div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden;">{lhead}<div style="display: flex; flex-direction: column; padding: 4px 0;">{lrows}</div></div>'
LIST = window(sidebar("home"), toolbar(["~"], "list", "Search Home") + list_body + statusbar("12 items · 1 selected"))


# ---------- context menu (icon view) ----------
def menu_item(label, key="", danger=False, sep=False):
    c = RED if danger else FG
    k = f'<span style="color: {CM}; font-size: 11px;">{key}</span>' if key else ""
    top = f'border-top: 1px solid {LINE}; margin-top: 4px; padding-top: 4px;' if sep else ""
    return (f'<div style="{top}"><div style="display: flex; align-items: center; justify-content: space-between; gap: 24px; height: 26px; padding: 0 12px; color: {c};">'
            f'<span>{label}</span>{k}</div></div>')

CONTEXT_MENU = f"""<div style="position: absolute; left: 230px; top: 300px; display: flex; flex-direction: column; width: 232px; padding: 4px 0; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);">
  {menu_item("Open", "Enter")}
  {menu_item("Open with…")}
  {menu_item("Copy", "Ctrl+C", sep=True)}
  {menu_item("Cut", "Ctrl+X")}
  {menu_item("Paste", "Ctrl+V")}
  {menu_item("Move to…", sep=True)}
  {menu_item("Rename", "F2")}
  {menu_item("Compress…")}
  {menu_item("Extract here")}
  {menu_item("Extract to…")}
  {menu_item("Copy path")}
  {menu_item("Move to Trash", "Del", danger=True, sep=True)}
</div>"""

icon_body = f"""<div style="display: flex; flex-grow: 1; min-height: 0; overflow: hidden;">
  <div style="position: relative; flex-grow: 1; min-width: 0; padding: 18px; overflow: hidden;">
    <div style="display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 10px;">{grid}</div>
    {CONTEXT_MENU}
  </div>
  {inspector(it=WALLPAPERS, side=True)}
</div>"""
ICON = window(sidebar("home"), toolbar(["~"], "icon", "Search Home", inspector=True) + icon_body + statusbar("12 items · 1 selected"))

# ---------- undo toast (list view) ----------
UNDO_TOAST = f"""<div style="position: absolute; left: 50%; bottom: 44px; transform: translateX(-50%); display: flex; align-items: center; gap: 14px; height: 36px; padding: 0 6px 0 14px; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5); white-space: nowrap;">
  <span style="color: {FGD};">Moved <span style="color: {FG};">wallpapers.zip</span> to Trash</span>
  <div style="display: flex; align-items: center; gap: 8px; height: 26px; padding: 0 10px; background: {HL}; color: {BLUE}; font-weight: 600; border-radius: 2px;"><span>Undo</span><span style="font-size: 11px; font-weight: 400; color: {CM};">Ctrl+Z</span></div>
</div>"""
list_body = f'<div style="display: flex; flex-grow: 1; min-height: 0; overflow: hidden;"><div style="position: relative; display: flex; flex-direction: column; flex-grow: 1; min-width: 0; overflow: hidden;">{lhead}<div style="display: flex; flex-direction: column; padding: 4px 0;">{lrows}</div>{UNDO_TOAST}</div>{inspector(it=PROJECTS_IT, side=True)}</div>'
LIST = window(sidebar("home"), toolbar(["~"], "list", "Search Home", inspector=True) + list_body + statusbar("11 items · 1 selected"))


# ---------- Split view: local | remote ----------
def pane(badge_icon, badge_color, badge, path, items, focused, selected=None):
    head_fg = FG if focused else FGD
    line = BLUE if focused else LINE
    rows = ""
    for n, k, m, sz in items:
        icon, color = KIND[k]
        sel = n == selected
        bg = BLUE if sel else "transparent"; fg = BG if sel else FGD; ic = BG if sel else color; dim = BG if sel else CM
        rows += (f'<div style="display: grid; grid-template-columns: minmax(0, 1fr) 150px 70px; align-items: center; gap: 12px; height: 28px; padding: 0 12px; background: {bg}; color: {fg};">'
                 f'<div style="display: flex; align-items: center; gap: 8px; min-width: 0;">{ico(icon, color=ic)}<span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{n}</span></div>'
                 f'<span style="color: {dim}; white-space: nowrap;">{m}</span><span style="color: {dim}; text-align: right;">{sz}</span></div>')
    return f"""<div style="display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0; min-width: 0; overflow: hidden;">
  <div style="display: flex; align-items: center; gap: 8px; height: 34px; flex: none; padding: 0 12px; background: {BGD}; border-bottom: 2px solid {line}; box-sizing: border-box; white-space: nowrap; overflow: hidden;">
    <div style="display: flex; align-items: center; gap: 6px; padding: 2px 6px; border: 1px solid {GUT}; border-radius: 2px; font-size: 11px; color: {badge_color};">{ico(badge_icon, size=12, color=badge_color)}<span>{badge}</span></div>
    <span style="color: {head_fg}; overflow: hidden; text-overflow: ellipsis;">{path}</span>
  </div>
  <div style="display: flex; flex-direction: column; padding: 4px 0;">{rows}</div>
</div>"""

LOCAL_KIKI = [("design","folder","12 Sep 2026 23:54","—"),("src","folder","12 Sep 2026 14:02","—"),("Cargo.lock","lock","12 Sep 2026 11:40","38 KB"),
              ("Cargo.toml","toml","12 Sep 2026 11:40","612 B"),("PLAN.md","md","12 Sep 2026 23:58","4.1 KB"),("README.md","md","12 Sep 2026 14:02","1.1 KB")]
REMOTE_KIKI = [("design","folder","12 Sep 2026 23:55","—"),("src","folder","12 Sep 2026 14:03","—"),("Cargo.toml","toml","12 Sep 2026 11:41","612 B"),("README.md","md","12 Sep 2026 14:03","1.1 KB")]

def view_menu_open(active="mirror", hidden=False):
    def item(label, key, on, sep=False):
        top = f'border-top: 1px solid {LINE}; margin-top: 4px; padding-top: 4px;' if sep else ""
        check = f'<span style="width: 14px; color: {BLUE}; font-weight: 600;">{"✓" if on else ""}</span>'
        return (f'<div style="{top}"><div style="display: flex; align-items: center; gap: 4px; height: 26px; padding: 0 10px; color: {FG};">{check}<span style="flex-grow: 1;">{label}</span>'
                f'<span style="color: {CM}; font-size: 11px;">{key}</span></div></div>')
    return f"""<div style="position: absolute; right: 100px; top: 48px; z-index: 5; display: flex; flex-direction: column; width: 232px; padding: 4px 0; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);">
  {item("Icon view", "Ctrl+1", active == "icon")}{item("List view", "Ctrl+2", active == "list")}{item("Columns view", "Ctrl+3", active == "columns")}{item("Mirror view", "Ctrl+4", active == "mirror")}
  {item("Show hidden files", "Ctrl+H", hidden, sep=True)}
</div>"""

mirror_bar = f"""<div style="display: flex; align-items: center; gap: 10px; height: 44px; flex: none; padding: 0 14px; border-bottom: 1px solid {LINE}; box-sizing: border-box; font-size: 12px;">
  {ico("hdd", size=14, color=FGD)}<span style="color: {FGD};">~/Projects/kiki</span>
  <div style="display: flex; align-items: center; justify-content: center; width: 26px; height: 26px; border: 1px solid {GUT}; border-radius: 2px; color: {CM};" title="Swap sides">{ico("mirror", size=12)}</div>
  {ico("server", size=14, color=GREEN)}<span style="color: {FGD};">homelab · /srv/kiki</span>
  <span style="flex-grow: 1;"></span>
  <span style="display: flex; align-items: center; gap: 6px; color: {CM}; font-size: 11px;"><span style="width: 6px; height: 6px; border-radius: 3px; background: {YELLOW};"></span>3 changed locally · last mirrored 2 h ago</span>
  <div style="display: flex; align-items: center; gap: 10px; height: 32px; padding: 0 6px 0 12px; background: {BLUE}; color: {BG}; border-radius: 2px; font-weight: 600;" title="Mirror local → homelab (Ctrl+M)">
    {ico("arr-u", size=14, color=BG)}<span>Mirror to homelab</span>
    <span style="padding: 1px 6px; border: 1px solid rgba(26, 27, 38, 0.35); border-radius: 2px; font-size: 10px; font-weight: 400; color: {BG};">⌃M</span>
    <span style="display: flex; align-items: center; justify-content: center; width: 22px; height: 22px; margin-left: 2px; border-left: 1px solid rgba(26, 27, 38, 0.35); color: {BG};" title="Direction and options">{ico("chev-d", size=10, color=BG)}</span>
  </div>
</div>"""

split_body = f"""{mirror_bar}<div style="display: flex; flex-grow: 1; min-height: 0; overflow: hidden;">
  {pane("hdd", FGD, "local", "~/Projects/kiki", LOCAL_KIKI, focused=False, selected="PLAN.md")}
  <div style="width: 1px; flex: none; background: {LINE};"></div>
  {pane("server", GREEN, "homelab", "/srv/kiki", REMOTE_KIKI, focused=True)}
</div>"""
split_status = (f'<div style="display: flex; align-items: center; gap: 14px; height: 28px; flex: none; padding: 0 14px; border-top: 1px solid {LINE}; font-size: 11px; color: {CM}; box-sizing: border-box; overflow: hidden;">'
                f'<div style="display: flex; align-items: center; gap: 14px; overflow: hidden;">{"".join(key(k, l) for k, l in SPLIT_KEYS)}</div><span style="flex-grow: 1;"></span>'
                f'<span style="white-space: nowrap;">Uploading <span style="color: {FG};">PLAN.md</span></span>'
                f'<div style="width: 120px; height: 4px; flex: none; background: {HL}; border-radius: 2px; overflow: hidden;"><div style="width: 72%; height: 100%; background: {BLUE};"></div></div>'
                f'<span style="white-space: nowrap;">72% · 4.1 MB/s</span></div>')
SPLIT = window(sidebar("homelab"), toolbar(["homelab", "srv", "kiki"], "mirror", "Search homelab", merged=True) + view_menu_open("mirror") + split_body + split_status)


# ---------- Mirror workspace ----------
def mirror_header(direction="upload"):
    up = direction == "upload"
    def side(icon, color, title, path, align):
        return (f'<div style="display: flex; flex-direction: column; align-items: {align}; gap: 4px; width: 340px;">'
                f'{ico(icon, size=36, color=color, sw=1)}<span style="font-weight: 600; color: {FG};">{title}</span>'
                f'<span style="font-size: 11px; color: {CM}; white-space: nowrap;">{path}</span></div>')
    def arrow(icon, on):
        return (f'<div style="display: flex; align-items: center; justify-content: center; width: 32px; height: 32px; border-radius: 2px; '
                f'background: {HL if on else "transparent"}; color: {BLUE if on else GUT};">{ico(icon, size=20)}</div>')
    return f"""<div style="display: flex; align-items: center; justify-content: center; gap: 24px; height: 104px; flex: none; padding: 0 24px; border-bottom: 1px solid {LINE}; box-sizing: border-box;">
  {side("hdd", FGD, "local", "~/Projects/greyhorse-site", "flex-end")}
  <div style="display: flex; gap: 4px;">{arrow("arr-l", not up)}{arrow("arr-r", up)}</div>
  {side("server", GREEN, "homelab", "/srv/www/greyhorse", "flex-start")}
</div>"""

def btn(label, primary=False, disabled=False):
    if primary:
        return f'<div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; background: {BLUE}; color: {BG}; font-weight: 600; border-radius: 2px;">{label}</div>'
    c = GUT if disabled else FGD
    return f'<div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 14px; border: 1px solid {GUT}; color: {c}; border-radius: 2px;">{label}</div>'

def mirror_footer(left, right):
    return (f'<div style="display: flex; align-items: center; gap: 8px; height: 56px; flex: none; padding: 0 20px; border-top: 1px solid {LINE}; box-sizing: border-box;">'
            f'{left}<span style="flex-grow: 1;"></span>{right}</div>')

def mirror_window(body, footer, direction="upload", keys=None, status=""):
    ws = f'<div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0;">{mirror_header(direction)}{body}{footer}{statusbar(status, keys=keys or [])}</div>'
    return window(sidebar("homelab"), toolbar(["homelab", "srv", "www", "greyhorse"], "mirror", "Search homelab") + ws)

def select(value, width="260px", disabled=False):
    c = GUT if disabled else FG
    return (f'<div style="display: flex; align-items: center; gap: 8px; width: {width}; height: 30px; padding: 0 10px; background: {BGD}; border: 1px solid {GUT}; '
            f'border-radius: 2px; color: {c}; box-sizing: border-box;"><span style="flex-grow: 1;">{value}</span>{ico("chev-d", size=12, color=CM)}</div>')

def check_row(on, label, extra="", muted=False):
    c = CM if muted else FG
    return f'<div style="display: flex; align-items: center; gap: 10px; height: 30px; color: {c};">{checkbox(on)}<span>{label}</span>{extra}</div>'

# --- Configure ---
configure_body = f"""<div style="display: flex; flex-direction: column; gap: 14px; flex-grow: 1; min-height: 0; padding: 28px 0 0 0; align-self: center; width: 620px;">
  <div style="display: flex; align-items: center; gap: 12px; height: 30px;"><span style="width: 150px; color: {FGD};">Detect changes by</span>{select("Automatic (size + date)")}{ico("info", size=14, color=CM)}</div>
  <div style="display: flex; flex-direction: column; gap: 2px;">
    {check_row(False, "Delete files on the destination that aren't on the source")}
    {check_row(True, "Skip items matching the filter rules", f'<div style="display: flex; align-items: center; height: 26px; padding: 0 10px; margin-left: 6px; border: 1px solid {GUT}; border-radius: 2px; color: {FGD}; font-size: 12px;">Edit rules…</div>')}
    {check_row(False, "Only mirror files modified in the last", f'<div style="display: flex; gap: 6px; margin-left: 4px;">{select("7", width="70px", disabled=True)}{select("days", width="100px", disabled=True)}</div>', muted=True)}
  </div>
  <div style="display: flex; align-items: center; gap: 12px; padding-top: 6px;">
    <div style="display: flex; flex-direction: column; gap: 2px; flex-grow: 1;"><span style="color: {FG};">Modification date offset</span><span style="font-size: 11px; color: {CM};">Determined automatically from files present on both sides</span></div>
    {btn("Time offset…")}
  </div>
  <div style="display: flex; flex-direction: column; gap: 6px; padding: 14px 16px; margin-top: 8px; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px;">
    <span style="font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: {BLUE};">Plan</span>
    <span style="color: {FGD}; line-height: 1.5;">Mirror the <span style="color: {FG}; font-weight: 600;">local</span> folder to <span style="color: {FG}; font-weight: 600;">homelab</span>. New and changed files are copied; nothing is deleted. Files matching your filter rules are ignored.</span>
  </div>
</div>"""
MIRROR_CONFIGURE = mirror_window(configure_body, mirror_footer(btn("Cancel"), btn("Preflight", primary=True)), keys=[("Enter","preflight"),("Esc","cancel"),("← →","direction"),("Space","toggle option"),("?","all keys")], status="mirror · configure")

# --- Review ---
PLAN_ROWS = [
  ("new", "index.html", "6.2 KB"), ("new", "css", "—"), ("new", "css/site.css", "18.4 KB"), ("new", "css/print.css", "2.1 KB"),
  ("changed", "footer.js", "3.9 KB"), ("changed", "images/greyhorse-dark.png", "142 KB"), ("new", "images/omatach/dash.png", "512 KB"),
  ("new", "images/omatach/troublecodes.png", "488 KB"), ("new", "images/omatach/vehicleinfo.png", "501 KB"), ("changed", "images/nativity.png", "310 KB"),
  ("new", "docs", "—"), ("new", "docs/setup.md", "4.4 KB"), ("changed", "dockit.html", "9.1 KB"), ("changed", "images/relayscreen.png", "244 KB"),
  ("delete", "old/banner-2024.png", "88 KB"), ("equal", "images", "—"), ("equal", "images/dockitscreen.jpg", "201 KB"), ("equal", "images/ghs.png", "12 KB"),
]
def op_label(kind, done=False):
    if kind == "new":     return ico("arr-u", size=12, color=BLUE), "copy (new)", FGD
    if kind == "changed": return ico("arr-u", size=12, color=YELLOW), "copy (changed)", FGD
    if kind == "mkdir":   return ico("arr-u", size=12, color=BLUE), "mkdir", FGD
    if kind == "delete":  return ico("x", size=12, color=RED), "delete", RED
    return ico("equals", size=12, color=GUT), "unchanged", CM

def review_row(kind, path, size):
    if kind == "new" and size == "—": kind = "mkdir"
    icon, label, c = op_label(kind)
    cb = checkbox(kind != "equal") if kind != "equal" else f'<div style="width: 16px; height: 16px;"></div>'
    return (f'<div style="display: grid; grid-template-columns: 24px 150px minmax(0, 1fr) 90px; align-items: center; gap: 12px; height: 28px; padding: 0 16px; color: {FGD};">'
            f'{cb}<div style="display: flex; align-items: center; gap: 6px; color: {c};">{icon}<span>{label}</span></div>'
            f'<span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: {CM if kind == "equal" else FG};">{path}</span><span style="text-align: right; color: {CM};">{size}</span></div>')

def tabs(items, active):
    out = ""
    for label, n in items:
        on = label == active
        out += (f'<div style="display: flex; align-items: center; gap: 6px; height: 36px; padding: 0 2px; margin-bottom: -1px; color: {FG if on else CM}; border-bottom: 2px solid {BLUE if on else "transparent"};">'
                f'<span>{label}</span><span style="font-size: 11px; color: {CM};">{n}</span></div>')
    return f'<div style="display: flex; gap: 20px; flex: none; padding: 0 20px; border-bottom: 1px solid {LINE};">{out}</div>'

thead = (f'<div style="display: grid; grid-template-columns: 24px 150px minmax(0, 1fr) 90px; align-items: center; gap: 12px; height: 30px; padding: 0 16px; border-bottom: 1px solid {LINE}; '
         f'font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: {CM};"><span></span><span>Operation</span><span>Path</span><span style="text-align: right;">Size</span></div>')
review_body = f"""{tabs([("All", 24), ("New", 12), ("Changed", 5), ("Unchanged", 6), ("Delete", 1)], "All")}
<div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden;">{thead}<div style="display: flex; flex-direction: column; padding: 4px 0;">{"".join(review_row(*r) for r in PLAN_ROWS)}</div></div>"""
review_summary = f'<span style="font-size: 12px; color: {CM};"><span style="color: {FG};">17</span> copy · <span style="color: {RED};">1</span> delete · <span style="color: {FG};">2.3 MB</span> to transfer · 6 filtered out</span>'
MIRROR_REVIEW = mirror_window(review_body, mirror_footer(btn("Back") + f'<div style="width: 12px;"></div>' + review_summary, btn("Cancel") + btn("Save report…") + btn("Mirror", primary=True)), keys=[("Enter","mirror"),("Space","toggle row"),("1-5","filter tab"),("^S","save report"),("Esc","back"),("?","all keys")], status="mirror · review")

# --- Running ---
RUN_ROWS = [
  ("done", "mkdir", "css", "—"), ("done", "new", "index.html", "6.2 KB"), ("done", "new", "css/site.css", "18.4 KB"), ("done", "new", "css/print.css", "2.1 KB"),
  ("done", "changed", "footer.js", "3.9 KB"), ("skipped", "changed", "images/greyhorse-dark.png", "142 KB"),
  ("run", 72, "new", "images/omatach/dash.png", "512 KB"), ("run", 41, "new", "images/omatach/troublecodes.png", "488 KB"), ("run", 8, "new", "images/omatach/vehicleinfo.png", "501 KB"),
  ("queued", "changed", "images/nativity.png", "310 KB"), ("queued", "mkdir", "docs", "—"), ("queued", "new", "docs/setup.md", "4.4 KB"),
  ("queued", "changed", "dockit.html", "9.1 KB"), ("queued", "changed", "images/relayscreen.png", "244 KB"), ("queued", "delete", "old/banner-2024.png", "88 KB"),
]
def run_row(r):
    state = r[0]
    if state == "run": _, pct, kind, path, size = r
    else: _, kind, path, size = r
    icon, label, c = op_label(kind)
    if state == "done":
        status = f'<div style="display: flex; align-items: center; gap: 6px; color: {GREEN};">{ico("check", size=14)}<span style="font-size: 12px;">done</span></div>'; pc = FG
    elif state == "skipped":
        status = f'<div style="display: flex; align-items: center; gap: 6px; color: {YELLOW};">{ico("warn", size=14)}<span style="font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">skipped · permission denied</span></div>'; pc = FG
    elif state == "run":
        status = (f'<div style="display: flex; align-items: center; gap: 10px;"><div style="flex-grow: 1; height: 4px; background: {HL}; border-radius: 2px; overflow: hidden;"><div style="width: {pct}%; height: 100%; background: {BLUE};"></div></div>'
                  f'<span style="width: 34px; font-size: 12px; text-align: right; color: {FGD};">{pct}%</span></div>'); pc = FG
    else:
        status = f'<span style="font-size: 12px; color: {GUT};">queued</span>'; pc = CM; c = GUT if kind != "delete" else c
    return (f'<div style="display: grid; grid-template-columns: 150px minmax(0, 1fr) 80px 220px; align-items: center; gap: 12px; height: 28px; padding: 0 20px;">'
            f'<div style="display: flex; align-items: center; gap: 6px; color: {c};">{icon}<span>{label}</span></div>'
            f'<span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: {pc};">{path}</span><span style="text-align: right; color: {CM};">{size}</span>{status}</div>')

run_head = (f'<div style="display: grid; grid-template-columns: 150px minmax(0, 1fr) 80px 220px; align-items: center; gap: 12px; height: 30px; padding: 0 20px; border-bottom: 1px solid {LINE}; '
            f'font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: {CM};"><span>Operation</span><span>Path</span><span style="text-align: right;">Size</span><span>Status</span></div>')
run_summary = f"""<div style="display: flex; flex-direction: column; gap: 10px; flex: none; padding: 18px 20px 14px; border-bottom: 1px solid {LINE};">
  <div style="display: flex; align-items: center; gap: 12px;">
    <span style="font-weight: 600; color: {FG};">Mirroring local to homelab</span>
    <span style="flex-grow: 1;"></span>
    <span style="font-size: 12px; color: {CM};">5 of 15 items · 1.1 MB of 2.3 MB · 6.4 MB/s</span>
    <div style="display: flex; align-items: center; gap: 6px; padding: 2px 8px; border: 1px solid {GUT}; border-radius: 2px; font-size: 11px; color: {FGD};"><span>3 at a time</span>{ico("chev-d", size=10, color=CM)}</div>
  </div>
  <div style="height: 6px; background: {HL}; border-radius: 3px; overflow: hidden;"><div style="width: 46%; height: 100%; background: {BLUE};"></div></div>
</div>"""
running_body = f"""{run_summary}
<div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden;">{run_head}<div style="display: flex; flex-direction: column; padding: 4px 0;">{"".join(run_row(r) for r in RUN_ROWS)}</div></div>"""
run_note = f'<span style="font-size: 12px; color: {CM};">1 skipped so far · a mirror run is not undoable; re-run to converge</span>'
MIRROR_RUNNING = mirror_window(running_body, mirror_footer(run_note, btn("Cancel", primary=True)), keys=[("Esc","cancel run"),("+ -","concurrency"),("?","all keys")], status="mirror · running")


# ---------- Search everywhere ----------
RESULTS = [
  ("folder", "omarchy", "~/Projects", "12 Sep 2026 14:02", "—", "exact"),
  ("folder", "omarchy", "~/.config", "11 Sep 2026 08:12", "—", "exact"),
  ("pdf", "omarchy-cheatsheet.pdf", "~", "30 Aug 2026 12:05", "318 KB", "prefix"),
  ("toml", "omarchy.conf", "~/Projects/dotfiles/hypr", "9 Sep 2026 21:40", "1.8 KB", "prefix"),
  ("sh", "omarchy-tokyo.css", "~/Projects/waybar-themes", "4 Sep 2026 18:23", "3.2 KB", "prefix"),
  ("md", "omarchy-setup.md", "~/Documents/notes", "28 Aug 2026 10:47", "5.6 KB", "prefix"),
  ("jpg", "omarchy-wallpaper.jpg", "~/Pictures", "21 Aug 2026 19:22", "2.4 MB", "prefix"),
  ("md", "09-omarchy-integration.md", "~/Projects/kiki/docs/0.1.0", "13 Sep 2026 00:31", "2.9 KB", "sub"),
  ("sh", "install-omarchy.sh", "~/Projects/dotfiles/bin", "3 Sep 2026 08:20", "2.4 KB", "sub"),
  ("folder", "themes-omarchy", "~/Projects/waybar-themes/archive", "2 Aug 2026 20:11", "—", "sub"),
  ("png", "screenshot-omarchy-2026-09-12.png", "~/Pictures/screenshots", "12 Sep 2026 13:58", "1.2 MB", "sub"),
  ("md", "notes-omarchy-migration.md", "~/Documents/notes/archive", "14 Jul 2026 09:30", "11 KB", "sub"),
]
def hl(name, q):
    i = name.lower().find(q)
    if i < 0: return name
    return f'{name[:i]}<span style="color: {BLUE}; font-weight: 600;">{name[i:i+len(q)]}</span>{name[i+len(q):]}'

def result_row(kind, name, parent, mod, size, rank, selected=False):
    icon, color = KIND[kind]
    bg = BLUE if selected else "transparent"; fg = BG if selected else FG; ic = BG if selected else color; dim = BG if selected else CM
    n = hl(name, "omarchy") if not selected else name
    return (f'<div style="display: grid; grid-template-columns: minmax(0, 1fr) 150px 70px; align-items: center; gap: 12px; height: 44px; padding: 0 16px; background: {bg}; color: {fg};">'
            f'<div style="display: flex; align-items: center; gap: 10px; min-width: 0;">{ico(icon, size=20, color=ic, sw=1.25)}'
            f'<div style="display: flex; flex-direction: column; gap: 1px; min-width: 0;"><span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{n}</span>'
            f'<span style="font-size: 11px; color: {dim}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{parent}</span></div></div>'
            f'<span style="font-size: 12px; color: {dim}; white-space: nowrap;">{mod}</span><span style="font-size: 12px; color: {dim}; text-align: right;">{size}</span></div>')

search_head = (f'<div style="display: flex; align-items: center; gap: 12px; height: 34px; flex: none; padding: 0 16px; border-bottom: 1px solid {LINE}; font-size: 11px; color: {CM};">'
               f'<span><span style="color: {FG};">312</span> results for <span style="color: {FG};">omarchy</span> in <span style="color: {FG};">Everywhere</span></span>'
               f'<span style="flex-grow: 1;"></span><span>index refreshed 2 min ago · 1,204,318 names</span><span style="color: {GUT};">|</span><span>scopes: <span style="color: {FGD};">everywhere:</span> <span style="color: {FGD};">homelab:</span> <span style="color: {FGD};">nas:</span></span></div>')
search_rows = "".join(result_row(*r, selected=(i == 0)) for i, r in enumerate(RESULTS))
search_body = f'<div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden;">{search_head}<div style="display: flex; flex-direction: column; padding: 4px 0;">{search_rows}</div></div>'
SEARCH_KEYS = [("Enter","open"),("^Enter","reveal in folder"),("Tab","cycle scope"),("⌫","remove scope"),("↑ ↓","move"),("Esc","close"),("?","all keys")]
SEARCH = window(sidebar("home"), toolbar(["~"], "list", "", search=("omarchy", "everywhere")) + search_body + statusbar("312 results · 1 selected", keys=SEARCH_KEYS))


# ---------- Search this folder (filter in place) ----------
def lrow_hl(name, kind, mod, size, q):
    icon, color = KIND[kind]
    kinds = {"folder":"Folder","sh":"Shell script","md":"Markdown","pdf":"PDF document","png":"PNG image","zip":"Zip archive"}
    return (f'<div style="display: grid; grid-template-columns: minmax(0, 1fr) 160px 80px 120px; align-items: center; gap: 12px; height: 28px; padding: 0 12px; color: {FGD};">'
            f'<div style="display: flex; align-items: center; gap: 8px; min-width: 0;">{ico(icon, color=color)}<span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{hl(name, q)}</span></div>'
            f'<span style="color: {CM};">{mod}</span><span style="color: {CM}; text-align: right;">{size}</span><span style="color: {CM};">{kinds[kind]}</span></div>')
folder_rows = "".join(lrow_hl(n, k, m, sz, "do") for n, k, m, sz in HOME if "do" in n.lower())
folder_body = f'<div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden;">{lhead}<div style="display: flex; flex-direction: column; padding: 4px 0;">{folder_rows}</div></div>'
def scope_menu():
    def item(label, detail, on=False, icon=None, color=FGD):
        check = ico("check", size=12, color=BLUE) if on else '<span style="width: 12px;"></span>'
        ic = ico(icon, size=14, color=color) if icon else ""
        return (f'<div style="display: flex; align-items: center; gap: 10px; height: 30px; padding: 0 12px; color: {FG if on else FGD}; background: {HL if on else "transparent"};">'
                f'{check}{ic}<span style="flex-grow: 1;">{label}</span><span style="font-size: 11px; color: {CM};">{detail}</span></div>')
    return f"""<div style="position: absolute; left: 478px; top: 42px; display: flex; flex-direction: column; width: 320px; padding: 4px 0; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5); z-index: 2;">
  {item("This folder", "~", on=True, icon="folder", color=BLUE)}
  {item("Everywhere", "index · 1.2M names", icon="search", color=FGD)}
  <div style="height: 1px; margin: 4px 0; background: {LINE};"></div>
  {item("homelab", "sftp", icon="server", color=GREEN)}
  {item("nas", "ftps", icon="server", color=CYAN)}
  <div style="padding: 8px 12px 4px; border-top: 1px solid {LINE}; margin-top: 4px; font-size: 11px; color: {CM};">Or type a prefix: <span style="color: {FGD};">everywhere:</span> <span style="color: {FGD};">homelab:</span></div>
</div>"""

SEARCH_FOLDER = window(sidebar("home"), toolbar(["~"], "list", "", search=("do", "folder")) + scope_menu() + folder_body + statusbar("2 of 12 items match", keys=[("Enter","open"),("Tab","cycle scope"),("Esc","clear"),("?","all keys")]))


# ---------- Project mode: kiki | editor | agent ----------
def badge(letter, color):
    return f'<span style="width: 14px; text-align: center; font-size: 11px; font-weight: 600; color: {color};">{letter}</span>'

def tree_row(depth, name, kind, expanded=None, selected=False, git=None, dim=False):
    icon, color = KIND[kind]
    pad = 12 + depth * 16
    if expanded is None: chev = '<span style="width: 12px;"></span>'
    else: chev = ico("chev-d" if expanded else "chev-r", size=12, color=CM)
    bg = BLUE if selected else "transparent"; fg = BG if selected else (CM if dim else FG); ic = BG if selected else color
    b = badge(*git) if git and not selected else (badge(git[0], BG) if git else "")
    return (f'<div style="display: flex; align-items: center; gap: 6px; height: 26px; padding: 0 10px 0 {pad}px; background: {bg}; color: {fg};">'
            f'{chev}{ico(icon, size=14, color=ic)}<span style="flex-grow: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{name}</span>{b}</div>')

TREE = [
  (0, "docs", "folder", True), (1, "0.1.0", "folder", False), (1, "design", "folder", False),
  (0, "packaging", "folder", False),
  (0, "src", "folder", True, False, ("M", YELLOW)),
  (1, "vfs", "folder", False), (1, "json.rs", "sh"), (1, "main.rs", "sh", None, True, ("M", YELLOW)), (1, "proto.rs", "sh"),
  (1, "server.rs", "sh", None, False, ("M", YELLOW)), (1, "string_pool.rs", "sh", None, False, ("A", GREEN)),
  (0, "tests", "folder", False), (0, "target", "folder", False, False, None, True),
  (0, "Cargo.lock", "lock"), (0, "Cargo.toml", "toml"), (0, "README.md", "md"),
]
def tree_rows():
    out = ""
    for t in TREE:
        depth, name, kind = t[0], t[1], t[2]
        expanded = t[3] if len(t) > 3 else None
        selected = t[4] if len(t) > 4 else False
        git = t[5] if len(t) > 5 else None
        dim = t[6] if len(t) > 6 else False
        out += tree_row(depth, name, kind, expanded, selected, git, dim)
    return out

def kiki_project_window():
    return f"""<div style="display: flex; flex-direction: column; width: 320px; height: 760px; flex: none; background: {BG}; border: 2px solid {GUT}; box-sizing: border-box; overflow: hidden;">
  <div style="display: flex; align-items: center; gap: 8px; height: 44px; flex: none; padding: 0 12px; border-bottom: 1px solid {LINE};">
    {ico("folder", size=16, color=BLUE)}<span style="font-weight: 600; color: {FG};">kiki</span>
    <span style="display: flex; align-items: center; gap: 4px; padding: 1px 6px; border: 1px solid {GUT}; border-radius: 2px; font-size: 11px; color: {FGD};">{ico("mirror", size=10, color=CM)}main <span style="color: {CM};">↑2</span></span>
    <span style="flex-grow: 1;"></span>
    <span style="display: flex; align-items: center; height: 24px; padding: 0 8px; border: 1px solid {GUT}; border-radius: 2px; font-size: 11px; color: {FGD};">Leave</span>
  </div>
  <div style="display: flex; align-items: center; gap: 8px; height: 30px; margin: 8px 10px 4px; padding: 0 8px; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; color: {CM}; font-size: 12px;">{ico("search", size=12)}<span>Filter tree</span></div>
  <div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden; padding: 2px 0;">{tree_rows()}</div>
  {statusbar("", keys=[("Enter","open"),("Space","toggle"),("⌥Enter","to agent"),("Esc","leave")])}
</div>"""

def code_line(n, parts, current=False):
    bg = HL if current else "transparent"
    spans = "".join(f'<span style="color: {c};">{t}</span>' for t, c in parts)
    return (f'<div style="display: flex; height: 22px; align-items: center; background: {bg};"><span style="width: 44px; flex: none; text-align: right; padding-right: 12px; color: {FG if current else GUT};">{n}</span>'
            f'<span style="white-space: pre;">{spans}</span></div>')

RUST = [
  [("use ", PURPLE), ("std::os::unix::net::UnixListener;", FG)],
  [("use ", PURPLE), ("crate::proto::{Frame, Request};", FG)],
  [],
  [("pub fn ", PURPLE), ("serve", BLUE), ("(listener: ", FG), ("UnixListener", YELLOW), (") -> ", FG), ("io::Result", YELLOW), ("<()> {", FG)],
  [("    ", FG), ("for ", PURPLE), ("stream ", FG), ("in ", PURPLE), ("listener.incoming() {", FG)],
  [("        ", FG), ("let ", PURPLE), ("mut ", PURPLE), ("client = ", FG), ("Client", YELLOW), ("::new(stream?);", FG)],
  [("        ", FG), ("std::thread::scope(|s| {", FG)],
  [("            s.spawn(", FG), ("move ", PURPLE), ("|| client.run());", FG)],
  [("        });", FG)],
  [("    }", FG)],
  [("    ", FG), ("Ok", YELLOW), ("(())", FG)],
  [("}", FG)],
  [],
  [("impl ", PURPLE), ("Client", YELLOW), (" {", FG)],
  [("    ", FG), ("fn ", PURPLE), ("run", BLUE), ("(&", FG), ("mut ", PURPLE), ("self) {", FG)],
  [("        ", FG), ("while ", PURPLE), ("let ", PURPLE), ("Some", YELLOW), ("(frame) = self.read_frame() {", FG)],
  [("            ", FG), ("match ", PURPLE), ("frame {", FG)],
  [("                Frame::", FG), ("Json", YELLOW), ("(req) => self.dispatch(req),", FG)],
  [("                Frame::", FG), ("Binary", YELLOW), ("(bytes) => self.stream_in(bytes),", FG)],
  [("            }", FG)],
  [("        }", FG)],
  [("    }", FG)],
  [],
  [("    ", FG), ("fn ", PURPLE), ("dispatch", BLUE), ("(&", FG), ("mut ", PURPLE), ("self, req: ", FG), ("Request", YELLOW), (") {", FG)],
  [("        ", FG), ("match ", PURPLE), ("req.kind.as_str() {", FG)],
  [("            ", FG), ('"Ping"', GREEN), (" => self.reply(req.id, ", FG), ("json!", PURPLE), ("({})),", FG)],
  [("            ", FG), ('"Open"', GREEN), (" => self.open(req),", FG)],
  [("            _ => self.error(req.id, ", FG), ('"Unsupported"', GREEN), ("),", FG)],
  [("        }", FG)],
  [("    }", FG)],
  [("}", FG)],
]
def editor_window():
    lines = "".join(code_line(i + 1, parts, current=(i == 25)) for i, parts in enumerate(RUST))
    return f"""<div style="display: flex; flex-direction: column; flex-grow: 1; height: 760px; min-width: 0; background: {BGD}; border: 2px solid {BLUE}; box-sizing: border-box; overflow: hidden; font-size: 13px;">
  <div style="display: flex; align-items: center; gap: 16px; height: 28px; flex: none; padding: 0 12px; border-bottom: 1px solid {LINE}; font-size: 12px; color: {CM};"><span style="color: {FG};">src/main.rs</span><span>src/server.rs</span><span>Cargo.toml</span></div>
  <div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; padding: 8px 0; overflow: hidden;">{lines}</div>
  <div style="display: flex; align-items: center; gap: 12px; height: 26px; flex: none; padding: 0 12px; background: {HL}; font-size: 12px; color: {FGD};"><span style="padding: 0 8px; background: {BLUE}; color: {BG}; font-weight: 600;">NORMAL</span><span>main</span><span>src/main.rs</span><span style="flex-grow: 1;"></span><span>rust</span><span>26:13</span><span>utf-8</span></div>
</div>"""

def agent_window():
    def line(t, c=FGD): return f'<div style="white-space: pre-wrap; color: {c};">{t}</div>'
    body = "".join([
      line("> Add a Ping handler to src/server.rs and wire it into dispatch.", FG), line(""),
      line("I'll read server.rs and proto.rs first.", FGD), line(""),
      line("  Read src/server.rs", CM), line("  Read src/proto.rs", CM), line(""),
      line("The dispatch match in main.rs already routes \"Ping\"; I'll add", FGD),
      line("the handler in server.rs and a test that opens the socket and", FGD),
      line("expects an empty ok reply.", FGD), line(""),
      line("  Edit src/server.rs  +14 −0", GREEN), line("  Edit tests/proto.rs  +22 −0", GREEN), line(""),
      line("Running cargo test…", FGD), line("  test proto::ping ... ok", GREEN), line(""),
      line("Done. Ping now replies {} and the test covers it.", FG),
    ])
    return f"""<div style="display: flex; flex-direction: column; width: 560px; height: 760px; flex: none; background: {BGD}; border: 2px solid {GUT}; box-sizing: border-box; overflow: hidden; font-size: 12px; line-height: 1.5;">
  <div style="display: flex; align-items: center; gap: 10px; height: 28px; flex: none; padding: 0 12px; border-bottom: 1px solid {LINE}; color: {CM};"><span style="color: {FG};">claude</span><span>~/Projects/kiki</span><span style="flex-grow: 1;"></span><span>main</span></div>
  <div style="display: flex; flex-direction: column; flex-grow: 1; min-height: 0; padding: 12px 14px; overflow: hidden;">{body}</div>
  <div style="display: flex; align-items: center; gap: 8px; height: 40px; flex: none; margin: 0 12px 12px; padding: 0 10px; border: 1px solid {GUT}; border-radius: 2px; color: {FG};"><span style="color: {BLUE};">›</span><span>Now add the Version handler<span style="display: inline-block; width: 1px; height: 14px; margin-left: 1px; background: {FG}; vertical-align: -2px;"></span></span></div>
</div>"""

PROJECT = (f'<div style="display: flex; gap: 8px; width: 2200px; height: 776px; padding: 8px; background: {BGD}; box-sizing: border-box;">'
           f'{kiki_project_window()}{editor_window()}{agent_window()}</div>')

# ---------- Add location dialogs ----------
def dfield(label, value, placeholder=False, icon=None, width="100%"):
    c = CM if placeholder else FG
    ic = ico(icon, size=14, color=CM) if icon else ""
    return (f'<div style="display: flex; flex-direction: column; gap: 6px; width: {width}; box-sizing: border-box;"><span style="font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; color: {CM};">{label}</span>'
            f'<div style="display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 10px; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; color: {c}; box-sizing: border-box;">{ic}<span style="flex-grow: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{value}</span></div></div>')

def dialog(kind, fields_html, height):
    def seg(label, on):
        bg = HL if on else "transparent"; c = FG if on else CM; w = "600" if on else "400"
        return f'<div style="display: flex; align-items: center; justify-content: center; flex-grow: 1; height: 28px; background: {bg}; color: {c}; font-weight: {w}; border-radius: 2px;">{label}</div>'
    return f"""<div style="display: flex; flex-direction: column; gap: 20px; width: 520px; height: {height}px; padding: 20px 24px 24px; background: {BG}; border: 2px solid {BLUE}; box-sizing: border-box;">
  <div style="display: flex; align-items: center; justify-content: space-between;">
    <span style="font-size: 15px; font-weight: 600; color: {FG};">Add location</span>
    <div style="display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; color: {CM};">{ico("x", size=14)}</div>
  </div>
  <div style="display: flex; gap: 2px; padding: 2px; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px;">{seg("SFTP", kind == "sftp")}{seg("FTPS", kind == "ftps")}</div>
  <div style="display: flex; flex-direction: column; gap: 14px;">{fields_html}</div>
  <div style="flex-grow: 1;"></div>
  <div style="display: flex; align-items: center; gap: 8px;">
    <span style="flex-grow: 1; font-size: 11px; color: {CM};">Secrets are kept in the Omarchy keyring.</span>
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; border: 1px solid {GUT}; color: {FGD}; border-radius: 2px;">Cancel</div>
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; background: {BLUE}; color: {BG}; font-weight: 600; border-radius: 2px;">Connect</div>
  </div>
</div>"""

sftp_fields = (dfield("Name", "homelab") +
    f'<div style="display: flex; gap: 12px;">{dfield("Host", "homelab.local", icon="server")}{dfield("Port", "22", width="110px")}</div>' +
    dfield("Username", "david") +
    dfield("Identity file", "~/.ssh/id_ed25519", icon="key") +
    dfield("Remote path", "/srv/kiki") +
    dfield("Local path", "~/Projects/kiki", icon="folder"))
SFTP = dialog("sftp", sftp_fields, 630)


def dselect(label, value, width="100%"):
    return (f'<div style="display: flex; flex-direction: column; gap: 6px; width: {width}; box-sizing: border-box;"><span style="font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; color: {CM};">{label}</span>'
            f'<div style="display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 10px; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; color: {FG}; box-sizing: border-box;"><span style="flex-grow: 1;">{value}</span>{ico("chev-d", size=12, color=CM)}</div></div>')

ftps_fields = (dfield("Name", "nas") +
    f'<div style="display: flex; gap: 12px;">{dfield("Host", "nas.local", icon="server")}{dfield("Port", "21", width="110px")}</div>' +
    f'<div style="display: flex; gap: 12px;">{dfield("Username", "david")}{dfield("Password", "••••••••••", placeholder=True)}</div>' +
    dselect("Encryption", "Explicit TLS (AUTH TLS)") +
    dfield("Remote path", "/volume1") +
    dfield("Local path", "~/nas", icon="folder"))
FTPS = dialog("ftps", ftps_fields, 630)



# ---------- Open dialog (xdg-desktop-portal FileChooser) ----------
def open_dialog():
    items = [(n, k) for n, k, _, _ in HOME if k in ("folder", "png")]
    rows = "".join(row(n, k, "active" if n == "screenshot-2026-09-12.png" else None) for n, k in items)
    side = f"""<div style="display: flex; flex-direction: column; width: 180px; flex: none; padding: 8px 0; background: {BGD}; border-right: 1px solid {LINE}; box-sizing: border-box;">
      {side_header("Favorites")}
      <div style="display: flex; flex-direction: column; gap: 1px;">{side_item("home", "Home", BLUE, True)}{side_item("download", "Downloads", BLUE)}{side_item("folder", "Pictures", BLUE)}{side_item("folder", "Projects", BLUE)}</div>
      {side_header("Locations")}
      <div style="display: flex; flex-direction: column; gap: 1px;">{side_item("hdd", "System", FGD)}{side_item("server", "homelab · sftp", GREEN)}</div>
    </div>"""
    return f"""<div style="display: flex; flex-direction: column; width: 860px; height: 560px; background: {BG}; border: 2px solid {BLUE}; box-sizing: border-box; overflow: hidden;">
  <div style="display: flex; align-items: center; gap: 8px; height: 48px; flex: none; padding: 0 12px; border-bottom: 1px solid {LINE}; box-sizing: border-box;">
    <span style="flex: none; padding-right: 8px; font-weight: 600; color: {FG};">Open File</span>
    <span style="flex: none; font-size: 11px; color: {CM};">requested by Signal</span>
    <div style="display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 10px; margin-left: 8px; flex-grow: 1; min-width: 0; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; white-space: nowrap; overflow: hidden;">{crumb("~", last=True)}</div>
    <div style="display: flex; align-items: center; gap: 8px; width: 220px; height: 30px; padding: 0 10px; flex: none; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px; color: {CM}; box-sizing: border-box;">{ico("search", size=14)}<span>Search Home</span></div>
  </div>
  <div style="display: flex; flex-grow: 1; min-height: 0;">
    {side}
    <div style="display: flex; flex-direction: column; flex-grow: 1; min-width: 0; padding: 6px 0;">{rows}</div>
  </div>
  <div style="display: flex; align-items: center; gap: 8px; height: 52px; flex: none; padding: 0 14px; border-top: 1px solid {LINE}; box-sizing: border-box;">
    <div style="display: flex; align-items: center; gap: 6px; height: 30px; padding: 0 10px; border: 1px solid {GUT}; border-radius: 2px; color: {FGD};"><span>Images</span>{ico("chev-d", size=12, color=CM)}</div>
    <span style="flex-grow: 1;"></span>
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; border: 1px solid {GUT}; color: {FGD}; border-radius: 2px;">Cancel</div>
    <div style="display: flex; align-items: center; justify-content: center; height: 30px; padding: 0 16px; background: {BLUE}; color: {BG}; font-weight: 600; border-radius: 2px;">Open</div>
  </div>
</div>"""
OPEN = open_dialog()


# ---------- Settings window (plan 20) ----------
def settings_nav(active):
    pages = ["General", "Keys", "Locations", "Search", "Open in", "Share", "Git", "Project mode", "Jarvis", "Plugins", "About"]
    rows = ""
    for pg in pages:
        on = pg == active
        rows += f'<div style="display: flex; align-items: center; height: 30px; margin: 0 8px; padding: 0 16px; border-radius: 2px; background: {HL if on else "transparent"}; color: {FG if on else FGD};">{pg}</div>'
    return f'<div style="display: flex; flex-direction: column; gap: 1px; width: 200px; flex: none; padding: 12px 0; background: {BGD}; border-right: 1px solid {LINE}; box-sizing: border-box;">{rows}</div>'

def setting_row(label, control, hint=""):
    h = f'<span style="font-size: 11px; color: {CM};">{hint}</span>' if hint else ""
    return (f'<div style="display: flex; align-items: center; gap: 16px; min-height: 40px;">'
            f'<div style="display: flex; flex-direction: column; gap: 2px; width: 220px; flex: none;"><span style="color: {FG};">{label}</span>{h}</div>{control}</div>')

def toggle(on):
    return (f'<div style="display: flex; align-items: center; width: 34px; height: 18px; padding: 2px; box-sizing: border-box; border-radius: 9px; background: {BLUE if on else GUT}; justify-content: {"flex-end" if on else "flex-start"};">'
            f'<div style="width: 14px; height: 14px; border-radius: 7px; background: {BG if on else FGD};"></div></div>')

settings_general = f"""<div style="display: flex; flex-direction: column; flex-grow: 1; min-width: 0; padding: 20px 28px; gap: 4px; overflow: hidden;">
  <div style="display: flex; align-items: center; justify-content: space-between; height: 32px; margin-bottom: 8px;"><span style="font-size: 15px; font-weight: 600; color: {FG};">General</span><span style="font-size: 11px; color: {GREEN};">Saved</span></div>
  {setting_row("Default view", select("List", width="220px"))}
  {setting_row("Sort by", select("Name · ascending", width="220px"))}
  {setting_row("Folders first", toggle(True))}
  {setting_row("Show hidden files", toggle(False), "Ctrl+H toggles per window")}
  {setting_row("Inspector on by default", toggle(True))}
  {setting_row("Confirm before remote delete", toggle(True), "remote locations have no trash")}
  {setting_row("Theme", select("Follow Omarchy", width="220px"), "or pick one of the eight Omarchy themes")}
  {setting_row("Font size", select("13 px", width="120px"))}
  {setting_row("Editor", select("Neovim · nvim --listen", width="300px"), "used by e and the code viewer")}
  <div style="flex-grow: 1;"></div>
  <div style="display: flex; gap: 8px;">{btn("Reset all settings…")}<span style="flex-grow: 1;"></span><span style="font-size: 11px; color: {CM}; align-self: center;">~/.config/kiki/settings.toml · every control writes through immediately</span></div>
</div>"""

SETTINGS = f"""<div style="display: flex; width: 900px; height: 640px; background: {BG}; border: 2px solid {BLUE}; box-sizing: border-box; overflow: hidden;">
  {settings_nav("General")}{settings_general}
</div>"""

# ---------- Share sheet (plan 18) ----------
def share_target(icon, name, detail, color, online=True, selected=False):
    return (f'<div style="display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 12px; border-radius: 2px; background: {HL if selected else "transparent"}; color: {FG if online else CM};">'
            f'{ico(icon, size=14, color=color)}<span style="flex-grow: 1;">{name}</span><span style="font-size: 11px; color: {CM};">{detail}</span>'
            f'<span style="width: 6px; height: 6px; border-radius: 3px; background: {GREEN if online else GUT};"></span></div>')

SHARE_SHEET = f"""<div style="position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.5);">
  <div style="display: flex; width: 640px; height: 420px; background: {BG}; border: 2px solid {BLUE}; box-sizing: border-box;">
    <div style="display: flex; flex-direction: column; width: 220px; flex: none; padding: 16px 0; background: {BGD}; border-right: 1px solid {LINE};">
      <span style="padding: 0 16px 10px; font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: {CM};">Share via</span>
      {share_target("phone", "LocalSend", "nearby", CYAN, selected=True)}
      {share_target("mail", "Mail", "SMTP", BLUE)}
      {share_target("message", "Messages", "KDE Connect · Signal", PURPLE)}
      {share_target("share", "Tailscale", "Taildrop", GREEN)}
      {share_target("share", "AirDrop", "needs owl", YELLOW, online=False)}
    </div>
    <div style="display: flex; flex-direction: column; flex-grow: 1; min-width: 0; padding: 16px 20px; gap: 10px;">
      <div style="display: flex; align-items: center; justify-content: space-between;"><span style="font-size: 15px; font-weight: 600; color: {FG};">Share 2 items</span><span style="font-size: 11px; color: {CM};">screenshot-2026-09-12.png, notes.md · 1.2 MB</span></div>
      <span style="font-size: 11px; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: {CM};">Devices nearby</span>
      <div style="display: flex; flex-direction: column; gap: 1px; padding: 4px; background: {BGD}; border: 1px solid {LINE}; border-radius: 2px;">
        {share_target("phone", "David's iPhone", "iOS", CYAN, selected=True)}
        {share_target("phone", "Pixel 8", "Android", CYAN)}
        {share_target("hdd", "studio-mac", "macOS", FGD)}
        {share_target("hdd", "192.168.1.40", "manual", FGD, online=False)}
      </div>
      {dfield("PIN (if the receiver asks)", "Optional", placeholder=True)}
      <div style="flex-grow: 1;"></div>
      <div style="display: flex; align-items: center; gap: 8px;"><span style="flex-grow: 1; font-size: 11px; color: {CM};">Progress shows in the Activity popover.</span>{btn("Cancel")}{btn("Send", primary=True)}</div>
    </div>
  </div>
</div>"""
share_main = f'<div style="position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden;">{lhead}<div style="display: flex; flex-direction: column; padding: 4px 0;">{lrows}</div>{SHARE_SHEET}</div>'
SHARE = window(sidebar("home"), toolbar(["~"], "list", "Search Home") + share_main + statusbar("12 items · 2 selected"))

# ---------- Jarvis panel (plan 19) ----------
def chat(role, text):
    if role == "user":
        return f'<div style="align-self: flex-end; max-width: 85%; padding: 8px 10px; background: {HL}; color: {FG}; border-radius: 2px; white-space: pre-wrap;">{text}</div>'
    return f'<div style="align-self: flex-start; max-width: 92%; padding: 8px 10px; color: {FGD}; white-space: pre-wrap; line-height: 1.5;">{text}</div>'

JARVIS_PANEL = f"""<div style="display: flex; flex-direction: column; width: 400px; flex: none; border-left: 1px solid {LINE}; background: {BG}; font-size: 12px;">
  <div style="display: flex; align-items: center; gap: 8px; height: 40px; flex: none; padding: 0 14px; border-bottom: 1px solid {LINE};">
    {ico("sparkle", size=14, color=PURPLE)}<span style="font-weight: 600; color: {FG};">Jarvis</span><span style="font-size: 11px; color: {CM};">claude · from Omarchy</span><span style="flex-grow: 1;"></span>
    <span style="font-size: 11px; color: {CM};">notes.md</span><span style="color: {CM};">{ico("x", size=12)}</span>
  </div>
  <div style="display: flex; flex-direction: column; gap: 10px; flex-grow: 1; min-height: 0; padding: 14px; overflow: hidden;">
    {chat("user", "how many times does 'omarchy' appear in this file?")}
    {chat("assistant", "12 times (lines 3, 8, 14, 21, 27, 33, 40, 41, 55, 62, 70, 88).")}
    {chat("user", "summarise the TODO section")}
    {chat("assistant", "Three open items: finish the SFTP fast-scan fallback, add a Trash view with Restore, and write the release workflow for aarch64. The first two are marked for this week; the third has no date.")}
    <span style="font-size: 11px; color: {CM};">count lines · word count · file size are answered locally; everything else runs <span style="color: {FGD};">claude -p</span> with the file attached</span>
  </div>
  <div style="display: flex; align-items: center; gap: 8px; height: 40px; flex: none; margin: 0 12px 12px; padding: 0 10px; border: 1px solid {BLUE}; border-radius: 2px; color: {FG};"><span style="color: {PURPLE};">›</span><span>Ask about notes.md<span style="display: inline-block; width: 1px; height: 14px; margin-left: 1px; background: {FG}; vertical-align: -2px;"></span></span><span style="flex-grow: 1;"></span><span style="font-size: 10px; padding: 1px 5px; border: 1px solid {GUT}; border-radius: 2px; color: {GUT};">Enter</span></div>
</div>"""
jarvis_main = f'<div style="display: flex; flex-grow: 1; min-height: 0; overflow: hidden;"><div style="display: flex; flex-direction: column; flex-grow: 1; min-width: 0; overflow: hidden;">{lhead}<div style="display: flex; flex-direction: column; padding: 4px 0;">{lrows}</div></div>{JARVIS_PANEL}</div>'
JARVIS = window(sidebar("home"), toolbar(["~"], "list", "Search Home") + jarvis_main + statusbar("12 items · 1 selected", keys=[("Enter","ask"),("Esc","close"),("^L","clear"),("⌥Q","toggle Jarvis"),("?","all keys")]))

# ---------- Trash view (plan 04) ----------
def trash_row(name, kind, original, deleted, size, selected=False):
    icon, color = KIND[kind]
    bg = BLUE if selected else "transparent"; fg = BG if selected else FG; dim = BG if selected else CM; ic = BG if selected else color
    return (f'<div style="display: grid; grid-template-columns: minmax(0, 1fr) 260px 150px 80px; align-items: center; gap: 12px; height: 28px; padding: 0 16px; background: {bg}; color: {fg};">'
            f'<span style="display: flex; align-items: center; gap: 8px; overflow: hidden;">{ico(icon, size=14, color=ic)}<span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{name}</span></span>'
            f'<span style="color: {dim}; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">{original}</span><span style="color: {dim};">{deleted}</span><span style="color: {dim}; text-align: right;">{size}</span></div>')

trash_head = (f'<div style="display: grid; grid-template-columns: minmax(0, 1fr) 260px 150px 80px; align-items: center; gap: 12px; height: 30px; padding: 0 16px; border-bottom: 1px solid {LINE}; '
              f'font-size: 11px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: {CM};"><span>Name</span><span>Original location</span><span>Deleted</span><span style="text-align: right;">Size</span></div>')
TRASH_ROWS = [("wallpapers.zip", "zip", "~", "12 Sep 2026 14:03", "84.5 MB", True), ("old-notes.md", "md", "~/Documents", "11 Sep 2026 09:40", "3.2 KB"), ("IMG_4021.jpg", "jpg", "~/Pictures/2026", "10 Sep 2026 22:15", "4.1 MB"),
              ("build", "folder", "~/Projects/kiki/target", "8 Sep 2026 17:02", "—"), ("draft.pdf", "pdf", "~/Downloads", "3 Sep 2026 08:31", "318 KB")]
TRASH_MENU = f"""<div style="position: absolute; left: 260px; top: 120px; display: flex; flex-direction: column; width: 232px; padding: 4px 0; background: {BGD}; border: 1px solid {GUT}; border-radius: 2px; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);">
  {menu_item("Restore to ~", "Enter")}
  {menu_item("Delete permanently", "Del", danger=True)}
  {menu_item("Copy path")}
  {menu_item("Empty Trash", danger=True, sep=True)}
</div>"""
trash_bar = f"""<div style="display: flex; align-items: center; gap: 10px; height: 40px; flex: none; padding: 0 16px; border-bottom: 1px solid {LINE}; box-sizing: border-box;">
  {ico("trash", size=14, color=FGD)}<span style="color: {FGD};">5 items · 89.1 MB · items are kept until you empty the trash</span><span style="flex-grow: 1;"></span>{btn("Restore")}{btn("Empty Trash…")}
</div>"""
trash_main = f'<div style="position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0; overflow: hidden;">{trash_bar}{trash_head}<div style="display: flex; flex-direction: column; padding: 4px 0;">{"".join(trash_row(*r) for r in TRASH_ROWS)}</div>{TRASH_MENU}</div>'
TRASH = window(sidebar("trash"), toolbar(["Trash"], "list", "Search Trash") + trash_main + statusbar("5 items · 1 selected", keys=[("Enter","restore"),("Del","delete permanently"),("^A","select all"),("?","all keys")]))

files = {"Main.dc.html": MAIN, "Settings.dc.html": SETTINGS, "ShareSheet.dc.html": SHARE, "Jarvis.dc.html": JARVIS, "TrashView.dc.html": TRASH, "OpenDialog.dc.html": OPEN, "InspectorPermissions.dc.html": INSPECTOR_PERMS, "IconView.dc.html": ICON, "ListView.dc.html": LIST, "SplitView.dc.html": SPLIT, "SearchEverywhere.dc.html": SEARCH, "SearchFolder.dc.html": SEARCH_FOLDER, "ProjectMode.dc.html": PROJECT, "MirrorConfigure.dc.html": MIRROR_CONFIGURE, "MirrorReview.dc.html": MIRROR_REVIEW, "MirrorRunning.dc.html": MIRROR_RUNNING, "AddLocationSFTP.dc.html": SFTP, "AddLocationFTPS.dc.html": FTPS}
for name, body in files.items():
    with open(os.path.join(OUT, name), "w") as f:
        f.write(doc(body))
print("wrote", ", ".join(files))
