import QtQuick
import QtQuick.Controls as QC
import Quickshell
import ".." as Kiki

// Settings (plan 20): a panel over the window, a page per area, every control writes through
// immediately. It is an overlay rather than a second window so it always sits above kiki itself.
Rectangle {
    id: sw
    visible: false
    anchors.fill: parent
    color: Qt.rgba(0, 0, 0, 0.5)
    z: 93
    property string page: "general"
    property var about: ({})
    property var index: ({})
    property var plugins: []
    property var sharePlugins: []
    property var pingResult: ({})
    property string flash: ""
    function open(p) { if (p) page = p; visible = true; reload(); panel.forceActiveFocus() }
    function close() { visible = false }
    function reload() {
        Kiki.Settings.load()
        Kiki.Daemon.request("About", {}, ok => { if (ok) sw.about = ok })
        Kiki.Daemon.request("IndexStatus", {}, ok => { if (ok) sw.index = ok })
        Kiki.Daemon.request("PluginStatus", {}, ok => { if (ok) sw.plugins = ok.plugins })
        Kiki.Daemon.request("SharePlugins", {}, ok => { if (ok) sw.sharePlugins = ok.plugins })
        Kiki.Daemon.request("Volumes", {}, ok => { if (ok) sw.volumes = ok.items })
    }
    function saved() { flash = "Saved"; flashTimer.restart() }
    Timer { id: flashTimer; interval: 1200; onTriggered: sw.flash = "" }
    function set(section, key, value) { Kiki.Settings.set(section, key, value); saved() }

    readonly property var pages: [
        { id: "general", label: "General" }, { id: "search", label: "Search" },
        { id: "share", label: "Share" }, { id: "git", label: "Git" }, { id: "project", label: "Project mode" }, { id: "ai", label: "Jarvis" }, { id: "omarchy", label: "Omarchy" }, { id: "about", label: "About" }
    ]

    MouseArea { anchors.fill: parent; onClicked: sw.close() }

    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: Math.min(900, parent.width - 40)
        height: Math.min(640, parent.height - 40)
        color: Kiki.Theme.bg; border.width: 1; border.color: Kiki.Theme.line
        clip: true
        focus: true
        Keys.onEscapePressed: sw.close()
        MouseArea { anchors.fill: parent }          // clicks in the panel never reach the scrim

        Item {
            id: titleBar
            width: parent.width; height: 36
            Text { x: 16; anchors.verticalCenter: parent.verticalCenter; text: "Settings"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 14; font.bold: true }
            Rectangle {
                width: 24; height: 24; radius: 2; anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                color: closeHover.containsMouse ? Kiki.Theme.surface : "transparent"
                Icon { anchors.centerIn: parent; name: "x"; size: 12; color: Kiki.Theme.fgDim }
                MouseArea { id: closeHover; anchors.fill: parent; hoverEnabled: true; onClicked: sw.close() }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
        }

    Row {
        anchors.fill: parent; anchors.topMargin: titleBar.height
        Rectangle {
            width: Math.min(170, Math.floor(parent.width * 0.28)); height: parent.height; color: Kiki.Theme.bgDark
            clip: true
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }
            Flickable {
                NaturalScroll { }
                anchors.fill: parent; anchors.topMargin: 12; anchors.bottomMargin: 28
                contentHeight: navCol.height; clip: true; boundsBehavior: Flickable.StopAtBounds
                Column {
                    id: navCol
                    width: parent.width; spacing: 1
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
            }
            Text { anchors.bottom: parent.bottom; anchors.bottomMargin: 12; x: 20; text: sw.flash; color: Kiki.Theme.green; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        }
        Flickable {
            NaturalScroll { }
            width: Math.max(0, parent.width - Math.min(170, Math.floor(parent.width * 0.28))); height: parent.height
            contentHeight: body.height + 48; contentWidth: width; clip: true; boundsBehavior: Flickable.StopAtBounds
            Column {
                id: body; x: 16; y: 24; width: parent.width - 32; spacing: 18
                Text { text: sw.pages.find(p => p.id === sw.page).label; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 16; font.bold: true }
                Loader { width: parent.width; sourceComponent: ({ general: general, search: search, share: sharePage, git: git, project: project, ai: ai, omarchy: omarchyPage, about: aboutPage })[sw.page] }
            }
        }
    }

    }

    // The label takes a share of the page width and the controls get the rest, so a narrow panel
    // still shows the controls instead of pushing them past its edge.
    component Row2: Row {
        property string label: ""
        property string hint: ""            // shown on hover rather than crowding the row
        readonly property real avail: parent ? parent.width : 400
        readonly property real labelWidth: Math.max(90, Math.min(240, avail * 0.55))
        width: avail; clip: true
        spacing: 14; height: Math.max(32, implicitHeight)
        Text { width: labelWidth; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter; text: label; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
        HoverHandler { id: rowHover }
        Tip { visible: hint !== "" && rowHover.hovered; text: hint }
    }
    component Choice: Rectangle {
        id: choice
        property var options: []; property string value: ""; signal picked(string v)
        width: parent && parent.avail !== undefined ? Math.max(110, Math.min(260, parent.avail - parent.labelWidth - 28)) : 260
        height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: drop.visible ? Kiki.Theme.accent : Kiki.Theme.gutter
        Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: choice.value; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
        Icon { anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; name: "chev-d"; size: 12; color: Kiki.Theme.muted }
        activeFocusOnTab: true
        Keys.onSpacePressed: drop.open()
        Keys.onReturnPressed: drop.open()
        // Up and Down step through the options without opening the list.
        Keys.onUpPressed: { const i = options.indexOf(value); if (i > 0) picked(options[i - 1]) }
        Keys.onDownPressed: { const i = options.indexOf(value); if (i >= 0 && i < options.length - 1) picked(options[i + 1]) }
        MouseArea { anchors.fill: parent; onClicked: drop.open() }
        // The list drops below the field. A Popup sits in the window's overlay, so it is not
        // clipped by the page's Flickable.
        QC.Popup {
            id: drop
            y: choice.height + 2; width: choice.width
            padding: 4
            background: Rectangle { color: Kiki.Theme.bgDark; radius: 2; border.width: 1; border.color: Kiki.Theme.gutter }
            contentItem: Column {
                Repeater {
                    model: choice.options
                    delegate: Rectangle {
                        required property string modelData
                        width: drop.availableWidth; height: 26; radius: 2
                        color: opt.containsMouse ? Kiki.Theme.surface : "transparent"
                        Text { x: 8; anchors.verticalCenter: parent.verticalCenter; text: modelData; color: modelData === choice.value ? Kiki.Theme.accent : Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        MouseArea { id: opt; anchors.fill: parent; hoverEnabled: true; onClicked: { choice.picked(modelData); drop.close() } }
                    }
                }
            }
        }
    }
    component Switch: Rectangle {
        property bool on: false; signal toggled()
        activeFocusOnTab: true
        border.width: activeFocus ? 2 : 0; border.color: Kiki.Theme.fg
        Keys.onSpacePressed: toggled()
        Keys.onReturnPressed: toggled()
        width: 36; height: 20; radius: 10; color: on ? Kiki.Theme.accent : Kiki.Theme.gutter
        Rectangle { width: 16; height: 16; radius: 8; y: 2; x: parent.on ? 18 : 2; color: Kiki.Theme.bg; Behavior on x { NumberAnimation { duration: 120 } } }
        MouseArea { anchors.fill: parent; onClicked: parent.toggled() }
    }
    component NumberBox: Rectangle {
        property int value: 0; property string error: ""; signal edited(int v)
        width: 100; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: error ? Kiki.Theme.red : Kiki.Theme.gutter
        TextInput { activeFocusOnTab: true; anchors.fill: parent; anchors.margins: 8; verticalAlignment: TextInput.AlignVCenter; text: parent.value; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; onEditingFinished: { const v = parseInt(text); if (isNaN(v) || v < 48 || v > 2000) parent.error = "48 to 2000"; else { parent.error = ""; parent.edited(v) } } }
        Text { anchors.left: parent.right; anchors.leftMargin: 8; anchors.verticalCenter: parent.verticalCenter; text: parent.error; color: Kiki.Theme.red; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    }

    Component { id: general; Column { spacing: 12
        Row2 { label: "Default view"; Choice { options: ["list", "icon", "columns", "gallery", "mirror"]; value: Kiki.Settings.view["default"]; onPicked: v => sw.set("view", "default", v) } }
        Row2 { label: "Sort by"; Choice { options: ["name", "kind", "size", "mtime", "atime"]; value: Kiki.Settings.view.sort; onPicked: v => sw.set("view", "sort", v) } }
        Row2 { hint: "h j k l move and e edits; off: typing jumps to a name (type-ahead), F4 edits"; label: "Vim keys"; Switch { on: Kiki.Settings.view.vimKeys === true; onToggled: sw.set("view", "vimKeys", !on) } }
        Row2 { hint: "Modified as \"yesterday 14:02\", \"3 h ago\"; off shows the full date"; label: "Relative dates"; Switch { on: Kiki.Settings.view.relativeDates !== false; onToggled: sw.set("view", "relativeDates", !on) } }
        Row2 { hint: "Rail is a column of icons that widens when you point at it; Traditional is the full panel"; label: "Favorites panel"
            Choice { options: ["rail", "traditional"]; value: Kiki.Settings.view.sidebarStyle || "rail"; onPicked: v => sw.set("view", "sidebarStyle", v) } }
        Row2 { hint: "off hides it until Ctrl+Shift+B"; label: "Show favorites panel"; Switch { on: Kiki.Settings.view.sidebar === true; onToggled: sw.set("view", "sidebar", !on) } }
        Row2 { hint: "default for new panes; Ctrl+H or the view menu toggles a pane"; label: "Show hidden files"; Switch { on: Kiki.Settings.view.showHidden === true; onToggled: sw.set("view", "showHidden", !on) } }
        Row2 { label: "Remember view per folder"; Switch { on: Kiki.Settings.view.rememberPerFolder !== false; onToggled: sw.set("view", "rememberPerFolder", !on) }
            Button { text: "Forget all"; onClicked: { Kiki.Daemon.request("ClearViewPrefs", {}); sw.saved() } } }
        Row2 { id: colsRow; label: "List columns"; Flow { width: Math.max(0, colsRow.avail - colsRow.labelWidth - 14); spacing: 12; anchors.verticalCenter: parent.verticalCenter
            Repeater { model: [["mtime", "Modified"], ["size", "Size"], ["kind", "Kind"], ["atime", "Accessed"]]
                delegate: Row { required property var modelData; spacing: 6
                    Switch { on: (Kiki.Settings.view.columns || []).indexOf(modelData[0]) >= 0; onToggled: { const cols = ["mtime", "size", "kind", "atime"].filter(c => c === modelData[0] ? !on : (Kiki.Settings.view.columns || []).indexOf(c) >= 0); sw.set("view", "columns", cols) } }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: modelData[1]; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } } } }
        Row2 { hint: "Accessed shows when a file was last read, with a heat colour fading over a year. \"filesystem\" uses the atime the mount keeps" + (sw.homeVolume ? " (this volume: " + sw.homeVolume.atimeSupport + (sw.homeVolume.atimeSupport === "relatime" ? ", updated at most once a day unless the file changed; strictatime gives minute accuracy" : (sw.homeVolume.atimeSupport === "noatime" ? ", never updated: choose kiki" : "")) + ")" : "") + ". \"kiki\" uses this app's own opens (Open, Open with, Open in, the viewer, Share) and falls back to atime with a hollow swatch."; label: "Heat source"; Choice { options: ["filesystem", "kiki"]; value: Kiki.Settings.view.heatSource || "filesystem"; onPicked: v => sw.set("view", "heatSource", v) }
            Button { text: "Clear access log"; onClicked: Kiki.Daemon.request("ClearAccessLog", {}, () => sw.saved()) } }
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
    property var integration: ({})
    property var volumes: []
    readonly property var homeVolume: { const home = "file://" + Quickshell.env("HOME"); let best = null; for (const v of volumes) if (v.uri && home.startsWith(v.uri.replace(/\/$/, "")) && (!best || v.uri.length > best.uri.length)) best = v; return best }
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
    Component { id: aboutPage; Column { spacing: 8
        Repeater { model: [["Version", sw.about.version], ["Socket", sw.about.socket], ["Plugins", sw.about.pluginDir], ["Config", sw.about.configDir]]; delegate: Row { required property var modelData; spacing: 14
            Text { width: 120; text: modelData[0]; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Text { width: Math.max(120, (parent.parent ? parent.parent.width : 520) - 134); wrapMode: Text.WrapAnywhere; text: modelData[1] || ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } }
        Item { width: 1; height: 10 }
        Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
        Item { width: 1; height: 2 }
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
        Item { width: 1; height: 10 }
        Button { text: "Reset all settings"; onClicked: Kiki.Daemon.request("ResetSettings", {}, () => sw.reload()) }
    } }
}
