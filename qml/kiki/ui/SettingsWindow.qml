import QtQuick
import QtQuick.Controls as QC
import Quickshell
import ".." as Kiki

// Settings (plan 20): a panel over the window, a page per area, every control writes through
// immediately. It is an overlay rather than a second window so it always sits above kiki itself.
Rectangle {
    id: sw
    /// The plugin rows' state column: the wider of the two words in the language, so the Ping
    /// buttons line up ("en marcha" is wider than "inactivo").
    FontMetrics { id: stateFont; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    readonly property int stateCol: Math.ceil(Math.max(56, stateFont.advanceWidth(Kiki.T.tr("settings.running")), stateFont.advanceWidth(Kiki.T.tr("settings.idle")))) + 4
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
        { id: "general", label: Kiki.T.tr("settings.page.general") }, { id: "search", label: Kiki.T.tr("settings.page.search") },
        { id: "share", label: Kiki.T.tr("settings.page.share") }, { id: "git", label: Kiki.T.tr("settings.page.git") }, { id: "project", label: Kiki.T.tr("settings.page.project") }, { id: "ai", label: Kiki.T.tr("settings.page.ai") }, { id: "omarchy", label: Kiki.T.tr("settings.page.omarchy") }, { id: "about", label: Kiki.T.tr("settings.page.about") }
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
            Text { x: 16; anchors.verticalCenter: parent.verticalCenter; text: Kiki.T.tr("settings.title"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 14; font.bold: true }
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
        // Up to 320: Spanish labels ran past 240 and were elided (2026-09-25); the control keeps `slot`.
        readonly property real labelWidth: Math.max(90, Math.min(320, avail * 0.55))
        /// What is left for the control beside the label: a field sized to this, and no wider
        /// than it wants, stays inside the page at half a window (owner, 2026-09-24: "settings
        /// panels are cut off when window is half sized").
        readonly property real slot: Math.max(110, avail - labelWidth - spacing)
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
        /// What the field does when it is left: a number in range is written through, anything
        /// else is refused where it was typed and nothing is sent. The typing calls this, so a
        /// test drives the same path a hand does.
        function commit(text) {
            const v = parseInt(text)
            if (isNaN(v) || v < 48 || v > 2000) error = "48 to 2000"
            else { error = ""; edited(v) }
        }
        width: 100; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: error ? Kiki.Theme.danger : Kiki.Theme.gutter
        TextInput { objectName: "number-input"; activeFocusOnTab: true; anchors.fill: parent; anchors.margins: 8; clip: true; verticalAlignment: TextInput.AlignVCenter; text: parent.value; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; onEditingFinished: parent.commit(text) }
        Text { anchors.left: parent.right; anchors.leftMargin: 8; anchors.verticalCenter: parent.verticalCenter; text: parent.error; color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    }

    Component { id: general; Column { spacing: 12
        Row2 { label: Kiki.T.tr("settings.fileIcons"); Choice { options: ["kiki", "system"]; value: Kiki.Settings.view.icons || "kiki"; onPicked: v => sw.set("view", "icons", v) } }
        Row2 { label: Kiki.T.tr("settings.defaultView"); Choice { objectName: "default-view"; options: ["list", "icon", "columns", "gallery"]; value: Kiki.Settings.view["default"] === "mirror" ? "list" : Kiki.Settings.view["default"]; onPicked: v => sw.set("view", "default", v) } }
        Row2 { label: Kiki.T.tr("settings.sortBy"); Choice { options: ["name", "kind", "size", "mtime", "atime"]; value: Kiki.Settings.view.sort; onPicked: v => sw.set("view", "sort", v) } }
        Row2 { hint: Kiki.T.tr("settings.shortcutChipsHint"); label: Kiki.T.tr("settings.shortcutChips"); Switch { objectName: "shortcut-chips"; on: Kiki.Settings.view.shortcutChips === true; onToggled: sw.set("view", "shortcutChips", !on) } }
        Row2 { hint: Kiki.T.tr("settings.relativeDatesHint"); label: Kiki.T.tr("settings.relativeDates"); Switch { on: Kiki.Settings.view.relativeDates !== false; onToggled: sw.set("view", "relativeDates", !on) } }
        Row2 { hint: Kiki.T.tr("settings.favoritesPanelHint"); label: Kiki.T.tr("settings.favoritesPanel")
            Choice { options: ["rail", "traditional"]; value: Kiki.Settings.view.sidebarStyle || "rail"; onPicked: v => sw.set("view", "sidebarStyle", v) } }
        Row2 { hint: Kiki.T.tr("settings.railGrowHint"); label: Kiki.T.tr("settings.railGrow"); Switch { objectName: "rail-hover"; on: Kiki.Settings.view.railHover !== false; onToggled: sw.set("view", "railHover", !on) } }
        Row2 { hint: Kiki.T.tr("settings.showFavoritesHint"); label: Kiki.T.tr("settings.showFavorites"); Switch { on: Kiki.Settings.view.sidebar === true; onToggled: sw.set("view", "sidebar", !on) } }
        Row2 { hint: Kiki.T.tr("settings.showHiddenHint"); label: Kiki.T.tr("settings.showHidden"); Switch { on: Kiki.Settings.view.showHidden === true; onToggled: sw.set("view", "showHidden", !on) } }
        Row2 { label: Kiki.T.tr("settings.rememberView"); Switch { on: Kiki.Settings.view.rememberPerFolder !== false; onToggled: sw.set("view", "rememberPerFolder", !on) }
            Button { text: Kiki.T.tr("settings.forgetAll"); onClicked: { Kiki.Daemon.request("ClearViewPrefs", {}); sw.saved() } } }
        Row2 { id: colsRow; label: Kiki.T.tr("settings.listColumns"); Flow { width: Math.max(0, colsRow.avail - colsRow.labelWidth - 14); spacing: 12; anchors.verticalCenter: parent.verticalCenter
            Repeater { model: [["mtime", Kiki.T.tr("column.modified")], ["size", Kiki.T.tr("column.size")], ["kind", Kiki.T.tr("column.kind")], ["atime", Kiki.T.tr("column.accessed")]]
                delegate: Row { required property var modelData; spacing: 6
                    Switch { on: (Kiki.Settings.view.columns || []).indexOf(modelData[0]) >= 0; onToggled: { const cols = ["mtime", "size", "kind", "atime"].filter(c => c === modelData[0] ? !on : (Kiki.Settings.view.columns || []).indexOf(c) >= 0); sw.set("view", "columns", cols) } }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: modelData[1]; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } } } }
        Row2 { hint: Kiki.T.tr("settings.heatSourceHint") + (sw.homeVolume ? Kiki.T.tr("settings.thisVolume", { support: sw.homeVolume.atimeSupport + (sw.homeVolume.atimeSupport === "relatime" ? Kiki.T.tr("settings.relatime") : (sw.homeVolume.atimeSupport === "noatime" ? Kiki.T.tr("settings.noatime") : "")) }) : "") + Kiki.T.tr("settings.heatKiki"); label: Kiki.T.tr("settings.heatSource"); Flow { id: heatFlow; width: parent.slot; spacing: 8; anchors.verticalCenter: parent.verticalCenter; Choice { width: 160; options: ["filesystem", "kiki"]; value: Kiki.Settings.view.heatSource || "filesystem"; onPicked: v => sw.set("view", "heatSource", v) }
            Button { text: Kiki.T.tr("settings.clearAccessLog"); onClicked: Kiki.Daemon.request("ClearAccessLog", {}, () => sw.saved()) } } }
    } }
    Component { id: search; Column { spacing: 12
        Text { text: Kiki.T.tr("settings.indexStatus", { n: sw.index.entries || 0, size: Kiki.Format.bytes(sw.index.bytes || 0) }) + (sw.index.refreshing ? Kiki.T.tr("settings.refreshing") : ""); color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Column { spacing: 4; Repeater { model: sw.index.roots || []; delegate: Text { required property string modelData; text: Kiki.Format.display(modelData, Quickshell.env("HOME")); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } }
        Row2 { label: Kiki.T.tr("settings.roots"); Rectangle { width: Math.min(420, parent.slot); height: 70; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            TextEdit { anchors.fill: parent; anchors.margins: 8; text: (sw.index.roots || []).map(r => Kiki.Format.display(r, Quickshell.env("HOME"))).join("\n"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12
                onEditingFinished: { const roots = text.split("\n").map(s => s.trim()).filter(s => s).map(s => "file://" + encodeURI(s.replace(/^~/, Quickshell.env("HOME")))); Kiki.Daemon.request("SetIndexRoots", { roots: roots }, () => { sw.saved(); sw.reload() }) } } } }
        Row2 { label: Kiki.T.tr("settings.excludes"); Rectangle { width: Math.min(420, parent.slot); height: 70; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            TextEdit { anchors.fill: parent; anchors.margins: 8; text: (Kiki.Settings.index && Kiki.Settings.index.excludes ? Kiki.Settings.index.excludes : [".cache", ".git", "node_modules", "__pycache__", ".Trash", "target"]).join("\n"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12
                onEditingFinished: { const ex = text.split("\n").map(s => s.trim()).filter(s => s); Kiki.Daemon.request("SetSettings", { patch: { index: { excludes: ex } } }, () => { Kiki.Settings.load(); Kiki.Daemon.request("IndexRebuild", {}); sw.saved() }) } } } }
        Button { text: Kiki.T.tr("settings.rebuildIndex"); onClicked: { Kiki.Daemon.request("IndexRebuild", {}); sw.saved() } }
    } }
    property var integration: ({})
    property var volumes: []
    readonly property var homeVolume: { const home = "file://" + Quickshell.env("HOME"); let best = null; for (const v of volumes) if (v.uri && home.startsWith(v.uri.replace(/\/$/, "")) && (!best || v.uri.length > best.uri.length)) best = v; return best }
    function loadIntegration() { Kiki.Daemon.request("Integration", {}, ok => { if (ok) sw.integration = ok }) }
    Component { id: omarchyPage; Column { spacing: 12
        Component.onCompleted: sw.loadIntegration()
        Text { width: parent.width; wrapMode: Text.WordWrap; text: Kiki.T.tr("settings.omarchyIntro"); color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Repeater {
            model: [
                { id: "mime", label: Kiki.T.tr("settings.mime"), file: sw.integration.mimeapps },
                { id: "dbus", label: Kiki.T.tr("settings.dbus"), file: sw.integration.services },
                { id: "hypr", label: Kiki.T.tr("settings.hypr"), file: sw.integration.bindings },
                { id: "portal", label: Kiki.T.tr("settings.portal"), file: sw.integration.portals },
            ]
            delegate: Row2 { required property var modelData; label: modelData.label
                Text { width: 36; anchors.verticalCenter: parent.verticalCenter; text: sw.integration[modelData.id] ? Kiki.T.tr("settings.on") : Kiki.T.tr("settings.off"); color: sw.integration[modelData.id] ? Kiki.Theme.green : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Button { id: act; text: sw.integration[modelData.id] ? Kiki.T.tr("settings.remove") : Kiki.T.tr("settings.apply"); onClicked: Kiki.Daemon.request(sw.integration[modelData.id] ? "Unintegrate" : "Integrate", { parts: [modelData.id] }, ok => { if (ok) { sw.integration = ok.status; const r = ok.results[0]; sw.flash = r.ok ? r.message : "Failed: " + r.message; flashTimer.restart() } }) }
                Text { width: Math.max(0, parent.slot - 36 - act.width - 2 * parent.spacing); elide: Text.ElideMiddle; anchors.verticalCenter: parent.verticalCenter; text: modelData.file || ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        }
        Text { visible: !!sw.integration.hyprConfigErrors && sw.integration.hyprConfigErrors.length > 0; width: parent.width; wrapMode: Text.WordWrap; text: Kiki.T.tr("settings.hyprErrors", { errors: (sw.integration.hyprConfigErrors || []).join("; ") }); color: Kiki.Theme.yellow; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        Flow { spacing: 8; width: parent.width
            Button { text: Kiki.T.tr("settings.makeDefault"); primary: true; onClicked: Kiki.Daemon.request("Integrate", {}, ok => { if (ok) { sw.integration = ok.status; sw.flash = ok.results.every(r => r.ok) ? Kiki.T.tr("settings.isDefault") : Kiki.T.tr("settings.stepsFailed", { which: ok.results.filter(r => !r.ok).map(r => r.part + ": " + r.message).join("; ") }); flashTimer.restart() } }) }
            Button { text: Kiki.T.tr("settings.removeFromOmarchy"); onClicked: Kiki.Daemon.request("Unintegrate", {}, ok => { if (ok) { sw.integration = ok.status; sw.flash = Kiki.T.tr("settings.integrationRemoved"); flashTimer.restart() } }) }
        }
    } }
    Component { id: git; Column { spacing: 12
        Row2 { label: Kiki.T.tr("settings.showGit"); Switch { on: Kiki.Settings.git.enabled !== false; onToggled: sw.set("git", "enabled", !on) } }
        Row2 { label: Kiki.T.tr("settings.ignoredFiles"); Choice { options: ["dim", "hide", "normal"]; value: Kiki.Settings.git.showIgnored; onPicked: v => sw.set("git", "showIgnored", v) } }
        Row2 { label: Kiki.T.tr("settings.folders"); Choice { options: ["aggregate", "off"]; value: Kiki.Settings.git.folders; onPicked: v => sw.set("git", "folders", v) } }
    } }
    Component { id: project; Column { spacing: 12
        Row2 { label: Kiki.T.tr("settings.treeWidth"); NumberBox { objectName: "project-width"; value: Kiki.Settings.project.width; onEdited: v => sw.set("project", "width", v) } }
        Row2 { label: Kiki.T.tr("settings.arrange"); Switch { on: Kiki.Settings.project.arrange !== false; onToggled: sw.set("project", "arrange", !on) } }
        Row2 { label: Kiki.T.tr("settings.agentSlot"); Switch { on: Kiki.Settings.project.agent !== false; onToggled: sw.set("project", "agent", !on) } }
    } }
    Component { id: ai; Column { spacing: 12
        property var status: ({})
        function refresh() { Kiki.Daemon.request("AiStatus", {}, ok => { if (ok) status = ok }) }
        Component.onCompleted: refresh()
        Text { text: status.configured ? Kiki.T.tr("settings.aiConfigured", { cli: status.cli, provider: status.provider }) : Kiki.T.tr("settings.aiNeeds", { cli: status.cli || Kiki.T.tr("settings.aTool"), provider: status.provider || "…" }); color: status.configured ? Kiki.Theme.green : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12; wrapMode: Text.WordWrap; width: parent.width }
        Row2 { label: Kiki.T.tr("settings.ai"); Choice { objectName: "ai-provider"; options: ["omarchy", "anthropic", "openai", "gemini", "xai", "custom"]; value: Kiki.Settings.jarvis.provider || "omarchy"; onPicked: v => Kiki.Daemon.request("AiConfigure", { provider: v }, () => { Kiki.Settings.load(); refresh(); sw.saved() }) } }
        Text { text: Kiki.T.tr("settings.omarchyAi", { which: status.omarchyProvider ? Kiki.T.tr("settings.currently", { provider: status.omarchyProvider }) : Kiki.T.tr("settings.noneDetected") }); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; wrapMode: Text.WordWrap; width: parent.width }
        Row2 { label: Kiki.T.tr("settings.customCommand"); Rectangle { width: Math.min(320, parent.slot); height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            TextInput { anchors.fill: parent; anchors.margins: 8; clip: true; verticalAlignment: TextInput.AlignVCenter; text: Kiki.Settings.jarvis.cliCommand || ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; onEditingFinished: Kiki.Daemon.request("AiConfigure", { cliCommand: text }, () => { Kiki.Settings.load(); refresh(); sw.saved() })
                Text { visible: !parent.text.length && !parent.activeFocus; text: Kiki.T.tr("settings.customExample"); color: Kiki.Theme.muted; font: parent.font; anchors.verticalCenter: parent.verticalCenter } } } }
        Text { text: Kiki.T.tr("settings.customHint"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; wrapMode: Text.WordWrap; width: parent.width }
    } }
    Component { id: sharePage; Column { spacing: 10
        Repeater { model: sw.sharePlugins; delegate: Column { required property var modelData; spacing: 6; width: parent.width
            Row { spacing: 14; height: 30
                Switch { anchors.verticalCenter: parent.verticalCenter; on: modelData.enabled !== false; onToggled: { Kiki.Daemon.request("ShareConfigure", { plugin: modelData.id, config: Object.assign({}, modelData.config || {}, { enabled: !on }), secrets: {} }, () => sw.reload()); sw.saved() } }
                Text { width: Math.min(200, Math.max(80, parent.parent.width * 0.4)); elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                Text { anchors.verticalCenter: parent.verticalCenter; text: Kiki.T.tr("settings.targets", { targets: modelData.targets, version: modelData.version }); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
            // The plugin's own form (mail's SMTP fields). The field is the inner repeater's
            // `modelData`, and the plugin is the outer one's, kept under a name of its own —
            // the delegate used to ask for a `fieldData` no Repeater supplies, so not one of
            // these fields was ever drawn: "Cannot create delegate", once a field, in the log.
            Repeater { id: fields; property var plugin: modelData; model: modelData.form || []; delegate: FormField { required property var modelData; readonly property var f: modelData; readonly property var plugin: fields.plugin; objectName: "share-field-" + f.key; width: 420; field: f; value: (plugin.config || {})[f.key] || f.default || ""; onEdited: v => { const cfg = Object.assign({}, plugin.config || {}); cfg[f.key] = v; const secrets = {}; if ((plugin.secretFields || []).includes(f.key)) { secrets[f.key] = v; delete cfg[f.key] } Kiki.Daemon.request("ShareConfigure", { plugin: plugin.id, config: cfg, secrets: secrets }, () => sw.saved()) } } }
        } }
        Text { visible: sw.sharePlugins.length === 0; text: Kiki.T.tr("settings.noSharePlugins"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
    } }
    Component { id: aboutPage; Column { spacing: 8
        Repeater { model: [[Kiki.T.tr("settings.version"), sw.about.version], [Kiki.T.tr("settings.socket"), sw.about.socket], [Kiki.T.tr("settings.plugins"), sw.about.pluginDir], [Kiki.T.tr("settings.config"), sw.about.configDir]]; delegate: Row { required property var modelData; spacing: 14
            Text { width: 120; text: modelData[0]; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Text { width: Math.max(120, (parent.parent ? parent.parent.width : 520) - 134); wrapMode: Text.WrapAnywhere; text: modelData[1] || ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } }
        Item { width: 1; height: 10 }
        Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
        Item { width: 1; height: 2 }
        Text { width: parent.width; wrapMode: Text.WordWrap; text: Kiki.T.tr("settings.pluginsIntro"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        Repeater { model: sw.plugins; delegate: Row { required property var modelData; spacing: 12; height: 30
            Rectangle { width: 8; height: 8; radius: 4; anchors.verticalCenter: parent.verticalCenter; color: modelData.running ? Kiki.Theme.green : Kiki.Theme.gutter }
            Text { width: Math.min(170, Math.max(90, parent.parent.width * 0.3)); elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter; text: modelData.name; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Text { width: 70; anchors.verticalCenter: parent.verticalCenter; text: modelData.kind; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { width: 80; anchors.verticalCenter: parent.verticalCenter; text: modelData.describe ? "v" + (modelData.describe.version || "?") : ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { width: sw.stateCol; anchors.verticalCenter: parent.verticalCenter; text: modelData.running ? Kiki.T.tr("settings.running") : Kiki.T.tr("settings.idle"); color: modelData.running ? Kiki.Theme.green : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Button { height: 24; text: Kiki.T.tr("settings.ping"); onClicked: Kiki.Daemon.request("PluginPing", { name: modelData.name }, (ok, err) => { const r = Object.assign({}, sw.pingResult); r[modelData.name] = ok ? ok.ms + " ms" : (err ? err.message : "?"); sw.pingResult = r }) }
            Text { anchors.verticalCenter: parent.verticalCenter; text: sw.pingResult[modelData.name] || ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 } } }
        Text { visible: sw.plugins.length === 0; text: Kiki.T.tr("settings.noPlugins", { dirs: sw.about.pluginDir || "" }); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12; wrapMode: Text.WrapAnywhere; width: parent.width }
        Item { width: 1; height: 10 }
        Button { objectName: "reset-all"; text: Kiki.T.tr("settings.resetAll"); onClicked: Kiki.Daemon.request("ResetSettings", {}, () => sw.reload()) }
    } }
}
