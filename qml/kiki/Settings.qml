pragma Singleton
import QtQuick
import "." as Kiki

// settings.toml as served by the daemon, with the same defaults it applies.
QtObject {
    id: settings
    property var view: ({ "default": "list", sort: "name", order: "asc", inspector: false, sidebar: true, sidebarStyle: "rail", rememberPerFolder: true, columns: ["mtime", "size", "kind"] })
    // Per-folder view memory (plan 02): uri -> { view, sort, order }
    property var viewPrefs: ({})
    function viewPref(uri) { return view.rememberPerFolder ? viewPrefs[uri] || null : null }
    function setViewPref(uri, v, sort, order, hidden) {
        if (!view.rememberPerFolder || !uri) return
        const p = Object.assign({}, viewPrefs); p[uri] = { view: v, sort: sort, order: order, hidden: hidden }; viewPrefs = p
        Kiki.Daemon.request("SetViewPref", { uri: uri, view: v, sort: sort, order: order, hidden: hidden })
    }
    function loadViewPrefs() { Kiki.Daemon.request("ViewPrefs", {}, ok => { if (ok) viewPrefs = ok.folders }) }
    property var timers: ({ toastMs: 8000, searchDebounceMs: 150, mirrorPollMs: 400 })
    property var editor: ({ terminal: "auto", placement: "right", tabWidth: 4 })
    property var git: ({ enabled: true, showIgnored: "dim", folders: "aggregate" })
    property var project: ({ width: 320, arrange: true, agent: true })
    property var jarvis: ({ provider: "omarchy", cliCommand: "" })
    property var index: ({ roots: [], excludes: [] })
    property var integration: ({ asked: false })
    /// Rebound shortcuts: action id -> chord. Empty means everything is on its default.
    property var keys: ({})
    property var mirror: ({ last: {} })
    property bool loaded: false

    function load() {
        Kiki.Daemon.request("Settings", {}, (ok, err) => {
            if (!ok) return
            for (const k of ["view", "timers", "editor", "git", "project", "jarvis", "index", "integration", "mirror", "keys"]) if (ok[k]) settings[k] = Object.assign({}, settings[k], ok[k])
            loaded = true
        })
        loadViewPrefs()
    }
    function set(section, key, value) {
        const patch = {}; patch[section] = {}; patch[section][key] = value
        settings[section] = Object.assign({}, settings[section], patch[section])
        Kiki.Daemon.request("SetSettings", { patch: patch })
    }
    property Connections c: Connections { target: Kiki.Daemon; function onReadyChanged() { if (Kiki.Daemon.ready) settings.load() } function onEvent(msg) { if (msg.event === "ViewPrefsChanged") settings.loadViewPrefs() } }
}
