import QtQuick
import Quickshell
import ".." as Kiki

// Settings (plan 20): one window, a page per area, every control writes through immediately.
FloatingWindow {
    id: sw
    title: "kiki settings"
    implicitWidth: 900
    implicitHeight: 640
    color: Kiki.Theme.bg
    visible: false
    property string page: "general"
    property var keymap: []
    property var about: ({})
    property var tools: []
    property var index: ({})
    property var locations: []
    property var plugins: []
    property var sharePlugins: []
    property var pingResult: ({})
    property string flash: ""
    function open(p) { if (p) page = p; visible = true; reload() }
    function reload() {
        Kiki.Settings.load()
        Kiki.Daemon.request("Keymap", {}, ok => { if (ok) keymap = ok.keys })
        Kiki.Daemon.request("About", {}, ok => { if (ok) about = ok })
        Kiki.Daemon.request("OpenInList", {}, ok => { if (ok) tools = ok.tools })
        Kiki.Daemon.request("IndexStatus", {}, ok => { if (ok) index = ok })
        Kiki.Daemon.request("Locations", {}, ok => { if (ok) locations = ok.locations })
        Kiki.Daemon.request("PluginStatus", {}, ok => { if (ok) plugins = ok.plugins })
        Kiki.Daemon.request("SharePlugins", {}, ok => { if (ok) sharePlugins = ok.plugins })
    }
    function saved() { flash = "Saved"; flashTimer.restart() }
    Timer { id: flashTimer; interval: 1200; onTriggered: sw.flash = "" }
    function set(section, key, value) { Kiki.Settings.set(section, key, value); saved() }

    readonly property var pages: [
        { id: "general", label: "General" }, { id: "keys", label: "Keys" }, { id: "locations", label: "Locations" }, { id: "search", label: "Search" },
        { id: "openin", label: "Open in" }, { id: "share", label: "Share" }, { id: "git", label: "Git" }, { id: "project", label: "Project mode" }, { id: "ai", label: "Jarvis" }, { id: "plugins", label: "Plugins" }, { id: "omarchy", label: "Omarchy" }, { id: "about", label: "About" }
    ]

    Row {
        anchors.fill: parent
        Rectangle {
            width: 200; height: parent.height; color: Kiki.Theme.bgDark
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }
            Column {
                anchors.fill: parent; anchors.topMargin: 12; spacing: 1
                Repeater {
                    model: sw.pages
                    delegate: Rectangle {
                        required property var modelData
                        width: parent.width - 16; x: 8; height: 30; radius: 2; color: sw.page === modelData.id ? Kiki.Theme.surface : "transparent"
                        Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: sw.page === modelData.id ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        MouseArea { anchors.fill: parent; onClicked: sw.page = modelData.id }
                    }
                }
            }
            Text { anchors.bottom: parent.bottom; anchors.bottomMargin: 12; x: 20; text: sw.flash; color: Kiki.Theme.green; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        }
        Flickable {
            width: parent.width - 200; height: parent.height; contentHeight: body.height + 48; clip: true
            Column {
                id: body; x: 28; y: 24; width: parent.width - 56; spacing: 18
                Text { text: sw.pages.find(p => p.id === sw.page).label; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 16; font.bold: true }
                Loader { width: parent.width; sourceComponent: { general: general, keys: keys, locations: locs, search: search, openin: openin, share: sharePage, git: git, project: project, ai: ai, plugins: pluginsPage, omarchy: omarchyPage, about: about }[sw.page] }
            }
        }
    }

    component Row2: Row { property string label: ""; spacing: 14; height: 32; Text { width: 220; anchors.verticalCenter: parent.verticalCenter; text: label; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize } }
    component Choice: Rectangle {
        property var options: []; property string value: ""; signal picked(string v)
        width: 260; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
        Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: value; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
        Icon { anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; name: "chev-d"; size: 12; color: Kiki.Theme.muted }
        MouseArea { anchors.fill: parent; onClicked: { const i = options.indexOf(value); picked(options[(i + 1) % options.length]) } }
    }
    component Switch: Rectangle {
        property bool on: false; signal toggled()
        width: 36; height: 20; radius: 10; color: on ? Kiki.Theme.accent : Kiki.Theme.gutter
        Rectangle { width: 16; height: 16; radius: 8; y: 2; x: parent.on ? 18 : 2; color: Kiki.Theme.bg; Behavior on x { NumberAnimation { duration: 120 } } }
        MouseArea { anchors.fill: parent; onClicked: parent.toggled() }
    }
    component NumberBox: Rectangle {
        property int value: 0; property string error: ""; signal edited(int v)
        width: 100; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: error ? Kiki.Theme.red : Kiki.Theme.gutter
        TextInput { anchors.fill: parent; anchors.margins: 8; verticalAlignment: TextInput.AlignVCenter; text: parent.value; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; onEditingFinished: { const v = parseInt(text); if (isNaN(v) || v < 48 || v > 2000) parent.error = "48 to 2000"; else { parent.error = ""; parent.edited(v) } } }
        Text { anchors.left: parent.right; anchors.leftMargin: 8; anchors.verticalCenter: parent.verticalCenter; text: parent.error; color: Kiki.Theme.red; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    }

    Component { id: general; Column { spacing: 12
        Row2 { label: "Default view"; Choice { options: ["list", "icon", "columns", "mirror"]; value: Kiki.Settings.view["default"]; onPicked: v => sw.set("view", "default", v) } }
        Row2 { label: "Sort by"; Choice { options: ["name", "kind", "size", "mtime", "atime"]; value: Kiki.Settings.view.sort; onPicked: v => sw.set("view", "sort", v) } }
        Row2 { label: "Pick the view by contents"; Switch { on: Kiki.Settings.view.smartView !== false; onToggled: sw.set("view", "smartView", !on) } Text { anchors.verticalCenter: parent.verticalCenter; text: "picture folders open in icon view, remote locations in Mirror view (memory wins)"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        Row2 { label: "Relative dates"; Switch { on: Kiki.Settings.view.relativeDates !== false; onToggled: sw.set("view", "relativeDates", !on) } Text { anchors.verticalCenter: parent.verticalCenter; text: "Modified as \"yesterday 14:02\", \"3 h ago\"; off shows the full date"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        Row2 { label: "Show hidden files"; Switch { on: Kiki.Settings.view.showHidden === true; onToggled: sw.set("view", "showHidden", !on) } Text { anchors.verticalCenter: parent.verticalCenter; text: "default for new panes; Ctrl+H or the view menu toggles a pane"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        Row2 { label: "Inspector on by default"; Switch { on: Kiki.Settings.view.inspector === true; onToggled: sw.set("view", "inspector", !on) } }
        Row2 { label: "Remember view per folder"; Switch { on: Kiki.Settings.view.rememberPerFolder !== false; onToggled: sw.set("view", "rememberPerFolder", !on) }
            Button { text: "Forget all"; onClicked: { Kiki.Daemon.request("ClearViewPrefs", {}); sw.saved() } } }
        Row2 { label: "List columns"; Row { spacing: 12; anchors.verticalCenter: parent.verticalCenter
            Repeater { model: [["mtime", "Modified"], ["size", "Size"], ["kind", "Kind"], ["atime", "Accessed"]]
                delegate: Row { required property var modelData; spacing: 6
                    Switch { on: (Kiki.Settings.view.columns || []).indexOf(modelData[0]) >= 0; onToggled: { const cols = ["mtime", "size", "kind", "atime"].filter(c => c === modelData[0] ? !on : (Kiki.Settings.view.columns || []).indexOf(c) >= 0); sw.set("view", "columns", cols) } }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: modelData[1]; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } } } }
        Row2 { label: ""; Text { width: 520; wrapMode: Text.WordWrap; text: "Accessed shows when a file was last read, with a heat colour fading over a year. On the default relatime mount option Linux updates it at most once a day unless the file changed; mount with strictatime for minute accuracy."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        Row2 { label: "Theme"; Choice { options: ["follow Omarchy", "Tokyo Night"]; value: Kiki.Settings.view.theme || "follow Omarchy"; onPicked: v => sw.set("view", "theme", v) } }
        Row2 { label: "Toast duration (ms)"; NumberBox { value: Kiki.Settings.timers.toastMs; onEdited: v => sw.set("timers", "toastMs", v) } }
    } }
    Component { id: keys; Column { spacing: 4
        Repeater { model: sw.keymap; delegate: Row { required property var modelData; spacing: 14; height: 24
            Text { width: 220; text: modelData.key; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Text { width: 380; text: modelData.action; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Text { text: "plan " + modelData.plan; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter } } }
        Text { text: "Rebinding is a later version."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    } }
    Component { id: locs; Column { spacing: 8
        Repeater { model: sw.locations; delegate: Row { required property var modelData; spacing: 14; height: 30
            Text { width: 200; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Text { width: 80; anchors.verticalCenter: parent.verticalCenter; text: modelData.plugin; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Text { width: 260; anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideMiddle; text: modelData.remoteUri; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Button { text: "Remove"; onClicked: Kiki.Daemon.request("RemoveLocation", { name: modelData.name }, () => sw.reload()) } } }
        Text { visible: sw.locations.length === 0; text: "No locations yet. Add one from the sidebar's + button."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
    } }
    Component { id: search; Column { spacing: 12
        Text { text: (sw.index.entries || 0).toLocaleString() + " names indexed · " + Kiki.Format.bytes(sw.index.bytes || 0) + (sw.index.refreshing ? " · refreshing" : ""); color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Column { spacing: 4; Repeater { model: sw.index.roots || []; delegate: Text { required property string modelData; text: Kiki.Format.display(modelData, Quickshell.env("HOME")); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } }
        Row2 { label: "Roots (one per line)"; Rectangle { width: 420; height: 70; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            TextEdit { anchors.fill: parent; anchors.margins: 8; text: (sw.index.roots || []).map(r => Kiki.Format.display(r, Quickshell.env("HOME"))).join("\n"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12
                onEditingFinished: { const roots = text.split("\n").map(s => s.trim()).filter(s => s).map(s => "file://" + encodeURI(s.replace(/^~/, Quickshell.env("HOME")))); Kiki.Daemon.request("SetIndexRoots", { roots: roots }, () => { sw.saved(); sw.reload() }) } } } }
        Row2 { label: "Excludes (names, one per line)"; Rectangle { width: 420; height: 70; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            TextEdit { anchors.fill: parent; anchors.margins: 8; text: (Kiki.Settings.index && Kiki.Settings.index.excludes ? Kiki.Settings.index.excludes : [".cache", ".git", "node_modules", "__pycache__", ".Trash", "target"]).join("\n"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12
                onEditingFinished: { const ex = text.split("\n").map(s => s.trim()).filter(s => s); Kiki.Daemon.request("SetSettings", { patch: { index: { excludes: ex } } }, () => { Kiki.Settings.load(); Kiki.Daemon.request("IndexRebuild", {}); sw.saved() }) } } } }
        Button { text: "Rebuild index"; onClicked: { Kiki.Daemon.request("IndexRebuild", {}); sw.saved() } }
    } }
    Component { id: openin; Column { spacing: 6
        Repeater { model: sw.tools; delegate: Row { required property var modelData; spacing: 14; height: 30
            Rectangle { width: 8; height: 8; radius: 4; anchors.verticalCenter: parent.verticalCenter; color: modelData.enabled ? Kiki.Theme.green : Kiki.Theme.gutter }
            Text { width: 160; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: modelData.enabled ? Kiki.Theme.fg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Text { width: 70; anchors.verticalCenter: parent.verticalCenter; text: modelData.role || ""; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { width: 300; anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideRight; text: modelData.enabled ? modelData.command : modelData.reason; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } } }
        Text { text: "Edit ~/.config/kiki/open-in.toml to add or override tools; the list reloads on save."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    } }
    property var integration: ({})
    function loadIntegration() { Kiki.Daemon.request("Integration", {}, ok => { if (ok) sw.integration = ok }) }
    Component { id: omarchyPage; Column { spacing: 12
        Component.onCompleted: sw.loadIntegration()
        Text { width: parent.width; wrapMode: Text.WordWrap; text: "Per-user integration with Omarchy. Each line can be applied or removed on its own; Remove all puts every file back the way it was."; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Repeater {
            model: [
                { id: "mime", label: "Default folder handler", file: sw.integration.mimeapps },
                { id: "dbus", label: "\"Show in folder\" (FileManager1)", file: sw.integration.services },
                { id: "hypr", label: "Hyprland keys and chooser rule", file: sw.integration.bindings },
                { id: "portal", label: "Open / Save dialogs (portal)", file: sw.integration.portals },
            ]
            delegate: Row2 { required property var modelData; label: modelData.label
                Text { width: 90; anchors.verticalCenter: parent.verticalCenter; text: sw.integration[modelData.id] ? "on" : "off"; color: sw.integration[modelData.id] ? Kiki.Theme.green : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Button { text: sw.integration[modelData.id] ? "Remove" : "Apply"; onClicked: Kiki.Daemon.request(sw.integration[modelData.id] ? "Unintegrate" : "Integrate", { parts: [modelData.id] }, ok => { if (ok) { sw.integration = ok.status; const r = ok.results[0]; sw.flash = r.ok ? r.message : "Failed: " + r.message; flashTimer.restart() } }) }
                Text { anchors.verticalCenter: parent.verticalCenter; text: modelData.file || ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        }
        Text { visible: sw.integration.hyprConfigErrors && sw.integration.hyprConfigErrors.length > 0; width: parent.width; wrapMode: Text.WordWrap; text: "Hyprland config errors: " + (sw.integration.hyprConfigErrors || []).join("; "); color: Kiki.Theme.yellow; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        Row { spacing: 8
            Button { text: "Make kiki the default"; primary: true; onClicked: Kiki.Daemon.request("Integrate", {}, ok => { if (ok) { sw.integration = ok.status; sw.flash = ok.results.every(r => r.ok) ? "kiki is the default" : "Some steps failed: " + ok.results.filter(r => !r.ok).map(r => r.part + ": " + r.message).join("; "); flashTimer.restart() } }) }
            Button { text: "Remove kiki from Omarchy"; onClicked: Kiki.Daemon.request("Unintegrate", {}, ok => { if (ok) { sw.integration = ok.status; sw.flash = "integration removed"; flashTimer.restart() } }) }
        }
    } }
    Component { id: git; Column { spacing: 12
        Row2 { label: "Show git status"; Switch { on: Kiki.Settings.git.enabled !== false; onToggled: sw.set("git", "enabled", !on) } }
        Row2 { label: "Ignored files"; Choice { options: ["dim", "hide", "normal"]; value: Kiki.Settings.git.showIgnored; onPicked: v => sw.set("git", "showIgnored", v) } }
        Row2 { label: "Folders"; Choice { options: ["aggregate", "off"]; value: Kiki.Settings.git.folders; onPicked: v => sw.set("git", "folders", v) } }
    } }
    Component { id: project; Column { spacing: 12
        Row2 { label: "Tree width (px)"; NumberBox { value: Kiki.Settings.project.width; onEdited: v => sw.set("project", "width", v) } }
        Row2 { label: "Arrange windows"; Switch { on: Kiki.Settings.project.arrange !== false; onToggled: sw.set("project", "arrange", !on) } }
        Row2 { label: "Agent slot"; Switch { on: Kiki.Settings.project.agent !== false; onToggled: sw.set("project", "agent", !on) } }
    } }
    Component { id: ai; Column { spacing: 12
        property var status: ({})
        function refresh() { Kiki.Daemon.request("AiStatus", {}, ok => { if (ok) status = ok }) }
        Component.onCompleted: refresh()
        Text { text: status.configured ? "Jarvis runs " + status.cli + " for " + status.provider + " (using its own login)" : "Jarvis needs " + (status.cli || "a command-line tool") + " for " + (status.provider || "…") + " on PATH, or a custom command"; color: status.configured ? Kiki.Theme.green : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12; wrapMode: Text.WordWrap; width: 560 }
        Row2 { label: "AI"; Choice { options: ["omarchy", "anthropic", "openai", "gemini", "xai", "custom"]; value: Kiki.Settings.jarvis.provider || "omarchy"; onPicked: v => Kiki.Daemon.request("AiConfigure", { provider: v }, () => { Kiki.Settings.load(); refresh(); sw.saved() }) } }
        Text { text: "omarchy = the AI in Omarchy's keybinding" + (status.omarchyProvider ? " (currently " + status.omarchyProvider + ")" : " (none detected; falls back to anthropic)") + ". Tools: claude, codex, gemini, grok, each in print mode."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; wrapMode: Text.WordWrap; width: 560 }
        Row2 { label: "Custom command"; Rectangle { width: 320; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            TextInput { anchors.fill: parent; anchors.margins: 8; verticalAlignment: TextInput.AlignVCenter; text: Kiki.Settings.jarvis.cliCommand || ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; onEditingFinished: Kiki.Daemon.request("AiConfigure", { cliCommand: text }, () => { Kiki.Settings.load(); refresh(); sw.saved() })
                Text { visible: !parent.text.length && !parent.activeFocus; text: "e.g. mytool --ask {prompt}"; color: Kiki.Theme.muted; font: parent.font; anchors.verticalCenter: parent.verticalCenter } } } }
        Text { text: "The command runs in the file's folder with {prompt} (the question, naming the files) and {files} substituted; its output streams into the Jarvis panel."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; wrapMode: Text.WordWrap; width: 560 }
    } }
    Component { id: sharePage; Column { spacing: 10
        Repeater { model: sw.sharePlugins; delegate: Column { required property var modelData; spacing: 6; width: parent.width
            Row { spacing: 14; height: 30
                Switch { anchors.verticalCenter: parent.verticalCenter; on: modelData.enabled !== false; onToggled: { Kiki.Daemon.request("ShareConfigure", { plugin: modelData.id, config: Object.assign({}, modelData.config || {}, { enabled: !on }), secrets: {} }, () => sw.reload()); sw.saved() } }
                Text { width: 200; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "targets: " + modelData.targets + " · v" + modelData.version; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
            Repeater { model: modelData.form || []; delegate: FormField { required property var fieldData; property var f: fieldData; width: 420; field: f; value: (modelData.config || {})[f.key] || f.default || ""; onEdited: v => { const cfg = Object.assign({}, modelData.config || {}); cfg[f.key] = v; const secrets = {}; if ((modelData.secretFields || []).includes(f.key)) { secrets[f.key] = v; delete cfg[f.key] } Kiki.Daemon.request("ShareConfigure", { plugin: modelData.id, config: cfg, secrets: secrets }, () => sw.saved()) } }
                property var fieldData: modelData }
        } }
        Text { visible: sw.sharePlugins.length === 0; text: "No share plugins found in the plugin directory."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
    } }
    Component { id: pluginsPage; Column { spacing: 6
        Text { text: "Every kiki-plugin-* binary found, with its kind and whether it is running now."; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        Repeater { model: sw.plugins; delegate: Row { required property var modelData; spacing: 12; height: 30
            Rectangle { width: 8; height: 8; radius: 4; anchors.verticalCenter: parent.verticalCenter; color: modelData.running ? Kiki.Theme.green : Kiki.Theme.gutter }
            Text { width: 170; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Text { width: 70; anchors.verticalCenter: parent.verticalCenter; text: modelData.kind; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { width: 80; anchors.verticalCenter: parent.verticalCenter; text: modelData.describe ? "v" + (modelData.describe.version || "?") : ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { width: 120; anchors.verticalCenter: parent.verticalCenter; text: modelData.running ? "running" : "idle"; color: modelData.running ? Kiki.Theme.green : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Button { height: 24; text: "Ping"; onClicked: Kiki.Daemon.request("PluginPing", { name: modelData.name }, (ok, err) => { const r = Object.assign({}, sw.pingResult); r[modelData.name] = ok ? ok.ms + " ms" : (err ? err.message : "?"); sw.pingResult = r }) }
            Text { anchors.verticalCenter: parent.verticalCenter; text: sw.pingResult[modelData.name] || ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 } } }
        Text { visible: sw.plugins.length === 0; text: "No plugins found. Directories: " + (sw.about.pluginDir || ""); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12; wrapMode: Text.WrapAnywhere; width: 560 }
    } }
    Component { id: about; Column { spacing: 8
        Repeater { model: [["Version", sw.about.version], ["Socket", sw.about.socket], ["Plugins", sw.about.pluginDir], ["Config", sw.about.configDir]]; delegate: Row { required property var modelData; spacing: 14
            Text { width: 120; text: modelData[0]; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Text { width: 520; wrapMode: Text.WrapAnywhere; text: modelData[1] || ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } }
        Item { width: 1; height: 8 }
        Button { text: "Reset all settings"; onClicked: Kiki.Daemon.request("ResetSettings", {}, () => sw.reload()) }
    } }
}
