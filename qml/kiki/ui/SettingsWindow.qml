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
    property string flash: ""
    function open(p) { if (p) page = p; visible = true; reload() }
    function reload() {
        Kiki.Settings.load()
        Kiki.Daemon.request("Keymap", {}, ok => { if (ok) keymap = ok.keys })
        Kiki.Daemon.request("About", {}, ok => { if (ok) about = ok })
        Kiki.Daemon.request("OpenInList", {}, ok => { if (ok) tools = ok.tools })
        Kiki.Daemon.request("IndexStatus", {}, ok => { if (ok) index = ok })
        Kiki.Daemon.request("Locations", {}, ok => { if (ok) locations = ok.locations })
    }
    function saved() { flash = "Saved"; flashTimer.restart() }
    Timer { id: flashTimer; interval: 1200; onTriggered: sw.flash = "" }
    function set(section, key, value) { Kiki.Settings.set(section, key, value); saved() }

    readonly property var pages: [
        { id: "general", label: "General" }, { id: "keys", label: "Keys" }, { id: "locations", label: "Locations" }, { id: "search", label: "Search" },
        { id: "openin", label: "Open in" }, { id: "git", label: "Git" }, { id: "project", label: "Project mode" }, { id: "ai", label: "AI" }, { id: "about", label: "About" }
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
                Loader { width: parent.width; sourceComponent: { general: general, keys: keys, locations: locs, search: search, openin: openin, git: git, project: project, ai: ai, about: about }[sw.page] }
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
        Row2 { label: "Default view"; Choice { options: ["list", "icon", "columns"]; value: Kiki.Settings.view["default"]; onPicked: v => sw.set("view", "default", v) } }
        Row2 { label: "Sort by"; Choice { options: ["name", "kind", "size", "mtime"]; value: Kiki.Settings.view.sort; onPicked: v => sw.set("view", "sort", v) } }
        Row2 { label: "Inspector on by default"; Switch { on: Kiki.Settings.view.inspector === true; onToggled: sw.set("view", "inspector", !on) } }
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
        Component.onCompleted: Kiki.Daemon.request("AiStatus", {}, ok => { if (ok) status = ok })
        Text { text: status.configured ? "Configured · source: " + status.source + " · model " + status.model : "Not configured"; color: status.configured ? Kiki.Theme.green : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Row2 { label: "Anthropic API key"; Rectangle { width: 320; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
            TextInput { id: keyInput; anchors.fill: parent; anchors.margins: 8; verticalAlignment: TextInput.AlignVCenter; echoMode: TextInput.Password; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize } }
            Button { text: "Save to keyring"; primary: true; onClicked: Kiki.Daemon.request("AiConfigure", { apiKey: keyInput.text }, (ok, err) => { keyInput.text = ""; if (ok) { sw.saved(); Kiki.Daemon.request("AiStatus", {}, ok2 => { if (ok2) status = ok2 }) } }) } }
        Text { text: "Or log in with the Claude CLI (ant auth login); kiki uses that credential when no key is set. Pricing: anthropic.com/pricing"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; wrapMode: Text.WordWrap; width: 560 }
    } }
    Component { id: about; Column { spacing: 8
        Repeater { model: [["Version", sw.about.version], ["Socket", sw.about.socket], ["Plugins", sw.about.pluginDir], ["Helpers", sw.about.helperDir], ["Config", sw.about.configDir]]; delegate: Row { required property var modelData; spacing: 14
            Text { width: 120; text: modelData[0]; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Text { width: 520; wrapMode: Text.WrapAnywhere; text: modelData[1] || ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 } } }
        Item { width: 1; height: 8 }
        Button { text: "Reset all settings"; onClicked: Kiki.Daemon.request("ResetSettings", {}, () => sw.reload()) }
    } }
}
