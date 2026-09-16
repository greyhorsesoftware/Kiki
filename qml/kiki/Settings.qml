pragma Singleton
import QtQuick
import "." as Kiki

// settings.toml as served by the daemon, with the same defaults it applies.
Singleton {
    id: settings
    property var view: ({ "default": "list", sort: "name", order: "asc", inspector: false })
    property var timers: ({ toastMs: 8000, searchDebounceMs: 150, mirrorPollMs: 400 })
    property var editor: ({ terminal: "auto", placement: "right", tabWidth: 4 })
    property var git: ({ enabled: true, showIgnored: "dim", folders: "aggregate" })
    property var project: ({ width: 320, arrange: true, agent: true })
    property var jarvis: ({ provider: "omarchy", model: "", baseUrl: "" })
    property bool loaded: false

    function load() {
        Kiki.Daemon.request("Settings", {}, (ok, err) => {
            if (!ok) return
            for (const k of ["view", "timers", "editor", "git", "project", "jarvis"]) if (ok[k]) settings[k] = Object.assign({}, settings[k], ok[k])
            loaded = true
        })
    }
    function set(section, key, value) {
        const patch = {}; patch[section] = {}; patch[section][key] = value
        settings[section] = Object.assign({}, settings[section], patch[section])
        Kiki.Daemon.request("SetSettings", { patch: patch })
    }
    property Connections c: Connections { target: Kiki.Daemon; function onReadyChanged() { if (Kiki.Daemon.ready) settings.load() } }
}
