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
    property color cyan: "#7dcfff"
    property color purple: "#bb9af7"
    property color green: "#9ece6a"
    property color yellow: "#e0af68"
    property color red: "#f7768e"

    /// The theme Omarchy says is current, for the settings page and for tests.
    property string name: ""
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
    property string fallbackFile: configDir + "alacritty.toml"

    property FileView omarchy: FileView {
        path: theme.themeFile
        watchChanges: true
        onLoaded: theme._applyColors(text())
        onFileChanged: reload()
    }
    /// Only used when there is no colors.toml: an older Omarchy, or a theme that predates it.
    property FileView legacy: FileView {
        path: theme.fallbackFile
        watchChanges: true
        onLoaded: if (!theme._haveColors) theme._applyAlacritty(text())
        onFileChanged: reload()
    }
    property bool _haveColors: false
    property FileView themeName: FileView {
        path: Quickshell.env("HOME") + "/.local/state/omarchy/current/theme.name"
        watchChanges: true
        onLoaded: theme.name = text().trim()
        onFileChanged: reload()
    }
    property Timer poll: Timer {
        interval: 2000; running: true; repeat: true
        onTriggered: { theme.omarchy.reload(); theme.themeName.reload(); if (!theme._haveColors) theme.legacy.reload() }
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
        if (n("blue")) accent = n("blue")
        if (n("cyan")) cyan = n("cyan")
        if (n("magenta")) purple = n("magenta")
        if (n("green")) green = n("green")
        if (n("yellow")) yellow = n("yellow")
        if (n("red")) red = n("red")
    }
}
