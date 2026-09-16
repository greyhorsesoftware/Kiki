import QtQuick
import ".." as Kiki

// Tabbed inspector: General (preview and fields) and Permissions. Reads through
// the daemon's Preview and Stat; chmod applies as a plan-04 job.
Rectangle {
    id: insp
    property string uri: ""
    property var row: null           // the listing row when known (name, kind, meta)
    property string tab: "general"   // code | general | permissions
    signal edit(string uri, int line)
    function isText() { const k = kind(); return k === "code" || k === "text" || (k === "document" && /\.(md|txt|rst|log|csv)$/i.test(name())) }
    property string home: ""
    property var preview: null
    property var meta: row ? row.meta : null
    property bool standalone: false
    signal open()
    signal chmod(int mode, bool recursive)

    color: Kiki.Theme.bg
    Rectangle { visible: !standalone; width: 1; height: parent.height; color: Kiki.Theme.line }

    onUriChanged: { preview = null; if (uri) { reload(); if (isText() && tab === "general") tab = "code"; if (!isText() && tab === "code") tab = "general" } }
    function reload() {
        const u = uri
        Kiki.Daemon.request("Preview", { uri: u }, (ok, err) => { if (u === insp.uri) preview = ok || null })
        if (!meta) Kiki.Daemon.request("Stat", { uri: u }, (ok, err) => { if (u === insp.uri && ok) insp.meta = ok })
    }
    function kind() { return row ? row.kind : "file" }
    function name() { return row ? row.name : decodeURIComponent(uri.split("/").pop()) }
    function symbolic(mode) {
        if (mode === undefined || mode === null) return "—"
        const t = row && row.isDir ? "d" : (row && row.isLink ? "l" : "-")
        let s = t
        for (const shift of [6, 3, 0]) { const b = (mode >> shift) & 7; s += (b & 4 ? "r" : "-") + (b & 2 ? "w" : "-") + (b & 1 ? "x" : "-") }
        return s
    }
    function octal(mode) { return mode === undefined || mode === null ? "—" : (mode & 0o777).toString(8).padStart(3, "0") }

    property int editMode: meta && meta.mode !== null && meta.mode !== undefined ? (meta.mode & 0o777) : 0
    onMetaChanged: editMode = meta && meta.mode !== null && meta.mode !== undefined ? (meta.mode & 0o777) : 0
    property bool dirty: meta && meta.mode !== null && meta.mode !== undefined && editMode !== (meta.mode & 0o777)

    Column {
        anchors.fill: parent; anchors.margins: 16; anchors.leftMargin: 20; spacing: 16
        // Header: icon, name, path
        Row {
            width: parent.width; spacing: 12
            Icon { name: insp.kind(); size: 40; strokeWidth: 1; color: Kiki.Theme.kindColor(insp.kind()) }
            Column {
                width: parent.width - 52; spacing: 4
                Text { width: parent.width; elide: Text.ElideRight; text: insp.name(); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
                Rectangle {
                    width: parent.width; height: 24; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line
                    Text { anchors.verticalCenter: parent.verticalCenter; x: 8; width: parent.width - 16; elide: Text.ElideMiddle; text: Kiki.Format.display(insp.uri, insp.home); color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                }
            }
        }
        // Tabs
        Item {
            width: parent.width; height: 32
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
            Row {
                spacing: 20; height: parent.height
                Repeater {
                    model: insp.isText() ? [{ id: "code", label: "Code" }, { id: "general", label: "General" }, { id: "permissions", label: "Permissions" }] : [{ id: "general", label: "General" }, { id: "permissions", label: "Permissions" }]
                    delegate: Item {
                        required property var modelData
                        width: t.implicitWidth + 4; height: 32
                        Text { id: t; anchors.centerIn: parent; text: modelData.label; color: insp.tab === modelData.id ? Kiki.Theme.fg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: insp.tab === modelData.id ? Kiki.Theme.accent : "transparent" }
                        MouseArea { anchors.fill: parent; onClicked: insp.tab = modelData.id }
                    }
                }
            }
        }
        Loader { width: parent.width; height: parent.height - 120; sourceComponent: insp.tab === "code" ? codeTab : (insp.tab === "general" ? general : permissions) }
    }
    Component { id: codeTab; CodeTab { uri: insp.uri; home: insp.home; onEdit: (u, line) => insp.edit(u, line) } }
    Item {
    }

    component Field: Row {
        property string label: ""
        property string value: ""
        property color valueColor: Kiki.Theme.fg
        spacing: 12; width: parent.width
        Text { width: 88; text: label; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Text { width: parent.width - 100; wrapMode: Text.WrapAnywhere; text: value; color: valueColor; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
    }

    Component {
        id: general
        Column {
            spacing: 18
            // Preview box
            Rectangle {
                width: parent.width; height: 150; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line; clip: true
                Text {
                    visible: insp.preview && insp.preview.text !== undefined
                    anchors.fill: parent; anchors.margins: 10
                    text: insp.preview && insp.preview.text !== undefined ? insp.preview.text : ""
                    color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11; lineHeight: 1.4; wrapMode: Text.NoWrap
                }
                Image {
                    visible: insp.preview && insp.preview.path !== undefined
                    anchors.fill: parent; anchors.margins: 6; fillMode: Image.PreserveAspectFit; asynchronous: true; smooth: true
                    source: insp.preview && insp.preview.path ? "file://" + insp.preview.path : ""
                    sourceSize: Qt.size(512, 512)
                }
                Column {
                    visible: insp.preview && insp.preview.members !== undefined
                    anchors.fill: parent; anchors.margins: 10
                    Repeater { model: insp.preview && insp.preview.members ? insp.preview.members.slice(0, 9) : []; delegate: Text { required property var modelData; text: (modelData.isDir ? "" : "  ") + modelData.name; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11; elide: Text.ElideMiddle; width: 250 } }
                }
                Column {
                    visible: insp.preview && insp.preview.children !== undefined
                    anchors.fill: parent; anchors.margins: 10
                    Repeater { model: insp.preview && insp.preview.children ? insp.preview.children.slice(0, 9) : []; delegate: Text { required property string modelData; text: modelData; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
                }
                Icon { visible: !insp.preview; anchors.centerIn: parent; name: insp.kind(); size: 48; strokeWidth: 1; color: Kiki.Theme.gutter }
            }
            Column {
                spacing: 8; width: parent.width
                Field { label: "Type"; value: Kiki.Format.kindLabel(insp.kind()) + (insp.preview && insp.preview.n !== undefined ? " · " + insp.preview.n + (insp.preview.members ? " members" : " items") : "") }
                Field { label: "Host"; value: insp.uri.startsWith("file://") ? "local" : insp.uri.split("://")[1].split("/")[0] }
                Field { label: "Location"; value: Kiki.Format.display(insp.uri.slice(0, insp.uri.lastIndexOf("/")) || insp.uri, insp.home) }
            }
            Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
            Column {
                spacing: 8; width: parent.width
                Field { label: "Size"; value: insp.meta ? (insp.row && insp.row.isDir ? "—" : Kiki.Format.bytes(insp.meta.size) + " (" + insp.meta.size.toLocaleString(Qt.locale(), "f", 0) + " bytes)") : "…" }
                Field { label: "Modified"; value: insp.meta ? Kiki.Format.date(insp.meta.mtime) : "…" }
                Field { label: "Owner"; value: insp.meta && insp.meta.owner ? insp.meta.owner : "—" }
                Field { label: "Group"; value: insp.meta && insp.meta.group ? insp.meta.group : "—" }
                Field { label: "Git"; value: insp.row && insp.row.git ? insp.row.git.state : "—"; valueColor: Kiki.Theme.yellow; visible: insp.row && insp.row.git }
            }
            Item { width: 1; height: 8 }
            Row {
                spacing: 8
                Button { text: "Open"; primary: true; onClicked: insp.open() }
                Button { text: "Open with…"; enabled: false }
            }
        }
    }

    Component {
        id: permissions
        Column {
            spacing: 14
            property bool canEdit: insp.meta && insp.meta.mode !== null && insp.meta.mode !== undefined
            Grid {
                columns: 4; columnSpacing: 0; rowSpacing: 2
                Item { width: 88; height: 24 }
                Repeater { model: ["Read", "Write", "Exec"]; delegate: Text { required property string modelData; width: 56; height: 24; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter; text: modelData.toUpperCase(); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true; font.letterSpacing: 0.6 } }
                Repeater {
                    model: [{ who: "Owner", shift: 6 }, { who: "Group", shift: 3 }, { who: "World", shift: 0 }]
                    delegate: Repeater {
                        required property var modelData
                        property var who: modelData
                        model: 4
                        delegate: Item {
                            required property int index
                            width: index === 0 ? 88 : 56; height: 28
                            Text { visible: index === 0; anchors.verticalCenter: parent.verticalCenter; text: who.who; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                            Rectangle {
                                visible: index > 0
                                property int bit: [0, 4, 2, 1][index]
                                property bool on: (insp.editMode >> who.shift) & bit
                                anchors.centerIn: parent; width: 16; height: 16; radius: 2
                                color: on ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: on ? Kiki.Theme.accent : Kiki.Theme.gutter
                                Icon { visible: parent.on; anchors.centerIn: parent; name: "check"; size: 10; strokeWidth: 2.5; color: Kiki.Theme.bg }
                                MouseArea { anchors.fill: parent; onClicked: if (canEdit) insp.editMode ^= (parent.bit << who.shift) }
                            }
                        }
                    }
                }
            }
            Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
            Column {
                spacing: 8; width: parent.width
                Field { label: "Octal"; value: canEdit ? insp.editMode.toString(8).padStart(3, "0") : "—" }
                Field { label: "Symbolic"; value: canEdit ? insp.symbolic(insp.editMode) : "—" }
                Field { label: "Owner"; value: insp.meta && insp.meta.owner ? insp.meta.owner : "—" }
                Field { label: "Group"; value: insp.meta && insp.meta.group ? insp.meta.group : "—" }
            }
            Row {
                spacing: 8
                property bool recursive: false
                Rectangle {
                    width: 16; height: 16; radius: 2; anchors.verticalCenter: parent.verticalCenter
                    color: parent.recursive ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: parent.recursive ? Kiki.Theme.accent : Kiki.Theme.gutter
                    MouseArea { anchors.fill: parent; onClicked: parent.parent.recursive = !parent.parent.recursive }
                }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "Apply to contained items"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            }
            Item { width: 1; height: 8 }
            Row {
                spacing: 8
                Button { text: "Apply"; primary: true; enabled: insp.dirty; onClicked: insp.chmod(insp.editMode, parent.parent.children[4].recursive) }
                Button { text: "Revert"; enabled: insp.dirty; onClicked: insp.editMode = insp.meta.mode & 0o777 }
            }
        }
    }
}
