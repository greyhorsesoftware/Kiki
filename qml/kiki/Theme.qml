pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

// Theme tokens. Defaults are Tokyo Night (the mockups); when Omarchy's current
// theme is present its alacritty palette is mapped onto the tokens and reloaded
// on change. `~/.config/omarchy/current` is a symlink Omarchy replaces, so the
// file watch is backed by a slow poll.
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
    property color cyan: "#7dcfff"
    property color purple: "#bb9af7"
    property color green: "#9ece6a"
    property color yellow: "#e0af68"
    property color red: "#f7768e"

    property string mono: "Cascadia Mono"
    property int fontSize: 13
    property int rowHeight: 28
    property int sidebarWidth: 224
    property int toolbarHeight: 48
    property int barHeight: 28
    property int inspectorWidth: 300

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

    property string themeFile: Quickshell.env("HOME") + "/.config/omarchy/current/theme/alacritty.toml"

    property FileView omarchy: FileView {
        path: theme.themeFile
        watchChanges: true
        onLoaded: theme._apply(text())
        onFileChanged: reload()
    }
    property Timer poll: Timer { interval: 5000; running: true; repeat: true; onTriggered: theme.omarchy.reload() }

    function _apply(text) {
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
        if (n("blue")) accent = n("blue")
        if (n("cyan")) cyan = n("cyan")
        if (n("magenta")) purple = n("magenta")
        if (n("green")) green = n("green")
        if (n("yellow")) yellow = n("yellow")
        if (n("red")) red = n("red")
    }
}
