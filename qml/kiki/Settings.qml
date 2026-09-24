pragma Singleton
import QtQuick
import "." as Kiki

// settings.toml as served by the daemon, with the same defaults it applies.
QtObject {
    id: settings
    property var view: ({ "default": "list", icons: "kiki", sort: "name", order: "asc", inspector: false, sidebar: true, sidebarStyle: "rail", railHover: true, rememberPerFolder: true, slideshowDelay: 4, slideshowLoop: true, columns: ["mtime", "size", "kind"], listColumnWidths: ({}) })
    // Per-folder view memory (plan 02): uri -> { view, sort, order }
    property var viewPrefs: ({})
    function viewPref(uri) { return view.rememberPerFolder ? viewPrefs[uri] || null : null }
    function setViewPref(uri, v, sort, order, hidden) {
        if (!view.rememberPerFolder || !uri) return
        const was = viewPrefs[uri]
        if (was && was.view === v && was.sort === sort && was.order === order && was.hidden === hidden) return      // nothing new to write
        const p = Object.assign({}, viewPrefs); p[uri] = { view: v, sort: sort, order: order, hidden: hidden }; viewPrefs = p
        _written[uri] = true
        Kiki.Daemon.request("SetViewPref", { uri: uri, view: v, sort: sort, order: order, hidden: hidden })
    }
    /// Prefs this window wrote and has not yet heard back about: the daemon answers a write with
    /// `ViewPrefsChanged` for the folder, and every window used to fetch ALL the prefs again for
    /// it — the writer included, which already knows (2026-09-24). Another window's change is
    /// still fetched.
    property var _written: ({})
    function onViewPrefsChanged(uri) {
        if (uri && _written[uri]) { delete _written[uri]; return }
        loadViewPrefs()
    }
    function loadViewPrefs() { Kiki.Daemon.request("ViewPrefs", {}, ok => { if (ok) viewPrefs = ok.folders }) }
    property var timers: ({ toastMs: 8000, searchDebounceMs: 150, mirrorPollMs: 400 })
    property var editor: ({ terminal: "auto", placement: "right" })
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
    /// Take one entry out of a map setting (`[view.listColumnWidths]`, `[view.columnsWidths]`).
    /// `set` cannot: the daemon merges maps, so a map sent without the entry leaves it in the
    /// file. `null` is how it is told to forget.
    function forget(section, key, entry) {
        const map = Object.assign({}, (settings[section] || ({}))[key] || ({}))
        delete map[entry]
        const local = {}; local[key] = map
        settings[section] = Object.assign({}, settings[section], local)
        const gone = {}; gone[entry] = null
        const patch = {}; patch[section] = {}; patch[section][key] = gone
        Kiki.Daemon.request("SetSettings", { patch: patch })
    }
    function set(section, key, value) {
        const patch = {}; patch[section] = {}; patch[section][key] = value
        settings[section] = Object.assign({}, settings[section], patch[section])
        Kiki.Daemon.request("SetSettings", { patch: patch })
    }
    property Connections c: Connections { target: Kiki.Daemon; function onReadyChanged() { if (Kiki.Daemon.ready) settings.load() } function onEvent(msg) { if (msg.event === "ViewPrefsChanged") settings.onViewPrefsChanged(msg.uri || "") } }
}
