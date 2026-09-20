pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "." as Kiki

// Theme tokens. Defaults are Tokyo Night (the mockups); when Omarchy's current theme is present
// its palette is mapped onto the tokens and reloaded on change.
//
// Omarchy publishes the active theme as a symlink at `~/.local/state/omarchy/current/theme`,
// whose `colors.toml` names every colour it uses. Older installations only had per-application
// files, so the alacritty palette is read as a fallback. Either way the symlink is replaced
// rather than edited when the theme changes, which no file watch sees, so a slow poll backs it.
Singleton {
    id: theme

    property color bg: "#1a1b26"
    property color bgDark: "#16161e"
    property color surface: "#292e42"
    property color line: "#292e42"
    property color gutter: "#3b4261"
    property color fg: "#c0caf5"
    property color fgDim: "#a9b1d6"
    property color muted: "#565f89"
    property color accent: "#7aa2f7"
    /// Navigational icons — sidebar, toolbar, breadcrumb — are all drawn in this, so they match
    /// each other and move together when the theme changes. Accent means "active", the status
    /// colours mean something is wrong or connected; neither is decoration.
    property color chrome: "#a9b1d6"
    property color cyan: "#7dcfff"
    property color purple: "#bb9af7"
    property color green: "#9ece6a"
    property color yellow: "#e0af68"
    property color red: "#f7768e"
    /// What "this is destructive" and "this went wrong" are drawn in. The theme's red — unless
    /// the theme's red is not red: a monochrome theme (hackerman: `red = "#50f872"`) names a
    /// green there, and "Move to Trash" in green reads as the safe choice. A palette slot is a
    /// colour; danger is a meaning, and it does not get to be green.
    readonly property color danger: isReddish(red) ? red : "#f7768e"
    function isReddish(c) {
        // Within ~30° of pure red on the wheel, and saturated enough to have a hue at all.
        const h = c.hslHue
        return c.hslSaturation >= 0.3 && (h < 0 ? false : (h <= 0.083 || h >= 0.917))
    }

    /// The theme Omarchy says is current, for the settings page and for tests.
    property string name: ""
    /// The desktop's icon theme: Omarchy names one per theme, and elsewhere GTK's setting is the
    /// nearest thing to a system answer. Empty means nobody said, so kiki draws its own.
    property string iconTheme: ""
    /// Theme-icon paths already resolved, keyed `name|size`, dropped when the theme changes.
    ///
    /// The daemon caches these too, but a round trip is a frame or three — long enough that every
    /// list rebuild (a file appears in the folder, a delegate is recycled) showed the placeholder
    /// icon first and the real one after, which reads as a flicker. Answered from here, a name
    /// resolved once is resolved instantly for the rest of the session.
    property var _iconPaths: ({})
    property var _iconWaiting: ({})
    /// Resolved lazily so a test can hand in a fake; the real singleton owns a socket.
    property var daemon: null
    function d() { return daemon || Kiki.Daemon }
    onIconThemeChanged: { _iconPaths = ({}); _iconWaiting = ({}) }

    /// The file for an icon name at a size: the path if it is known, "" if it is not. When it is
    /// not, `cb` is called with the path once the daemon answers — one request per key however
    /// many delegates ask at once.
    function iconPath(name, size, cb) {
        if (!iconTheme) return ""
        const key = name + "|" + size
        const hit = _iconPaths[key]
        if (hit !== undefined) return hit
        if (_iconWaiting[key]) { _iconWaiting[key].push(cb); return "" }
        _iconWaiting[key] = [cb]
        const asked = iconTheme
        d().request("Icon", { name: name, theme: asked, size: size }, ok => {
            if (asked !== theme.iconTheme) return          // a theme we have already left
            const p = ok && ok.path ? "file://" + ok.path : ""
            const m = Object.assign({}, theme._iconPaths); m[key] = p; theme._iconPaths = m
            const waiting = theme._iconWaiting[key] || []
            delete theme._iconWaiting[key]
            // One caller failing must not cost the others their answer: a delegate destroyed
            // while the request was in flight throws here, and at first launch (the list is
            // built, then rebuilt as the listing lands) that left every live row behind it on
            // kiki's own icon until the next rebuild.
            for (const f of waiting) {
                if (!f) continue
                try { f(p) } catch (e) { console.warn("Theme.iconPath: a waiting caller failed:", e) }
            }
        })
        return ""
    }
    property string mono: "Cascadia Mono"
    property int fontSize: 13
    property int rowHeight: 28
    property int sidebarWidth: 224
    property int toolbarHeight: 48
    property int barHeight: 28
    property int inspectorWidth: 360

    function kindColor(kind) {
        switch (kind) {
        case "folder": return accent
        case "image": return purple
        case "video": return purple
        case "audio": return cyan
        case "code": return green
        case "text": case "document": return fgDim
        case "pdf": return red
        case "archive": return yellow
        case "link": return cyan
        default: return fgDim
        }
    }

    readonly property string stateDir: Quickshell.env("HOME") + "/.local/state/omarchy/current/theme/"
    readonly property string configDir: Quickshell.env("HOME") + "/.config/omarchy/current/theme/"
    property string themeFile: stateDir + "colors.toml"
    /// Where an Omarchy without colors.toml keeps its palette: beside it today, under ~/.config on
    /// the older layout. Both are tried, quietly — neither need exist.
    property string fallbackFile: stateDir + "alacritty.toml"
    property string olderFallbackFile: configDir + "alacritty.toml"

    // None of the files under `current/theme` is watched: Omarchy switches theme by deleting that
    // folder and moving a new one into its place, so a watch on anything inside it is on a dead
    // inode after the first switch. What it does last is write `theme.name`, IN PLACE — same
    // inode, every time — so that one watch survives every switch and is the trigger for
    // re-reading the rest (`themeName` below). This replaced a timer that re-read four files
    // every two seconds, for ever, in every window (plan 30, W4).
    property FileView omarchy: FileView {
        path: theme.themeFile
        printErrors: false
        onLoaded: theme._applyColors(text())
    }
    /// Only used when there is no colors.toml: an older Omarchy, or a theme that predates it.
    property FileView legacy: FileView {
        path: theme.fallbackFile
        printErrors: false
        onLoaded: if (!theme._haveColors) theme._applyAlacritty(text())
    }
    property FileView olderLegacy: FileView {
        path: theme.olderFallbackFile
        printErrors: false
        onLoaded: if (!theme._haveColors) theme._applyAlacritty(text())
    }
    property bool _haveColors: false
    /// Omarchy publishes the icon theme beside the palette.
    property FileView iconsFile: FileView {
        path: theme.stateDir + "icons.theme"
        printErrors: false
        onLoaded: theme.iconTheme = text().trim()
    }
    /// Not on Omarchy: GTK's own setting, which every desktop writes.
    property FileView gtk4: FileView {
        path: Quickshell.env("HOME") + "/.config/gtk-4.0/settings.ini"
        printErrors: false
        onLoaded: theme._gtkIcons(text())
    }
    property FileView gtk3: FileView {
        path: Quickshell.env("HOME") + "/.config/gtk-3.0/settings.ini"
        printErrors: false
        onLoaded: theme._gtkIcons(text())
    }
    function _gtkIcons(text) {
        if (theme.iconTheme || !text) return           // Omarchy's answer wins where there is one
        const m = text.match(/^\s*gtk-icon-theme-name\s*=\s*(.+)$/m)
        if (m) theme.iconTheme = m[1].trim()
    }
    property FileView themeName: FileView {
        path: Quickshell.env("HOME") + "/.local/state/omarchy/current/theme.name"
        watchChanges: true
        printErrors: false
        // Truncated and then written: the first of the two events can find it empty.
        onLoaded: { const n = text().trim(); if (n) theme.name = n }
        onFileChanged: { reload(); theme.rereadTheme() }
    }
    /// The theme changed under us: read again everything that came from it.
    function rereadTheme() {
        theme._haveColors = false
        theme.omarchy.reload()
        theme.iconsFile.reload()
        theme.legacy.reload()
        theme.olderLegacy.reload()
    }

    /// Omarchy's own palette: one flat table of named colours.
    function _applyColors(text) {
        if (!text) return
        const c = {}
        for (const raw of text.split("\n")) {
            const m = raw.trim().match(/^(\w+)\s*=\s*["']?(#[0-9a-fA-F]{6})["']?/)
            if (m) c[m[1]] = m[2]
        }
        if (!c.background || !c.foreground) return
        _haveColors = true
        bg = c.background
        bgDark = c.dark_background || Qt.darker(c.background, 1.12)
        surface = c.lighter_background || Qt.lighter(c.background, 1.45)
        line = c.selection || Qt.lighter(c.background, 1.45)
        gutter = c.muted || Qt.lighter(c.background, 1.9)
        fg = c.bright_foreground || c.foreground
        fgDim = c.foreground
        muted = c.dark_foreground || c.muted || Qt.darker(c.foreground, 2.0)
        chrome = c.light_foreground || c.foreground
        accent = c.accent || c.blue || accent
        cyan = c.cyan || cyan
        purple = c.magenta || purple
        green = c.green || green
        yellow = c.yellow || yellow
        red = c.red || red
    }

    function _applyAlacritty(text) {
        if (!text) return
        // [colors.primary] background/foreground and [colors.normal] black..white
        const sec = {}
        let cur = ""
        for (const raw of text.split("\n")) {
            const l = raw.trim()
            if (l.startsWith("[")) { cur = l.slice(1, -1); continue }
            const m = l.match(/^(\w+)\s*=\s*["']?(#?[0-9a-fA-F]{6})["']?/)
            if (m) sec[cur + "." + m[1]] = "#" + m[2].replace("#", "")
        }
        const p = k => sec["colors.primary." + k], n = k => sec["colors.normal." + k], b = k => sec["colors.bright." + k]
        if (p("background")) bg = p("background")
        if (p("foreground")) fg = p("foreground")
        bgDark = Qt.darker(bg, 1.12)
        surface = Qt.lighter(bg, 1.45)
        line = Qt.lighter(bg, 1.45)
        gutter = Qt.lighter(bg, 1.9)
        fgDim = Qt.darker(fg, 1.15)
        muted = b("black") || n("black") || Qt.darker(fg, 2.0)
        chrome = Qt.darker(fg, 1.1)
        if (n("blue")) accent = n("blue")
        if (n("cyan")) cyan = n("cyan")
        if (n("magenta")) purple = n("magenta")
        if (n("green")) green = n("green")
        if (n("yellow")) yellow = n("yellow")
        if (n("red")) red = n("red")
    }
}
