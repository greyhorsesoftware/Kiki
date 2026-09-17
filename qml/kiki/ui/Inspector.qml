import QtQuick
import ".." as Kiki

// Tabbed inspector: General (preview and fields) and Permissions. Reads through
// the daemon's Preview and Stat; chmod applies as a plan-04 job.
Rectangle {
    id: insp
    property string uri: ""
    property var row: null           // the listing row when known (name, kind, meta)
    property string tab: "general"   // general | permissions
    signal edit(string uri, int line)
    property string home: ""
    property var preview: null
    property var meta: row ? row.meta : null
    property bool standalone: false
    signal openWith()
    signal open()
    signal chmod(int mode, bool recursive)

    color: Kiki.Theme.bg
    Rectangle { visible: !standalone; width: 1; height: parent.height; color: Kiki.Theme.line }

    onUriChanged: { preview = null; if (uri) reload() }
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
                    model: [{ id: "general", label: "General" }, { id: "permissions", label: "Permissions" }]
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
        Loader { width: parent.width; height: parent.height - 120; sourceComponent: insp.tab === "general" ? general : permissions }
    }
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
                readonly property bool markdown: insp.preview && insp.preview.markdown === true
                readonly property bool isImage: insp.preview && insp.preview.path !== undefined
                // An image preview takes the width of the panel and keeps its own ratio, rather
                // than sitting letterboxed in a short box.
                readonly property int imageHeight: img.implicitWidth > 0
                    ? Math.min(400, Math.round((width - 12) * img.implicitHeight / img.implicitWidth) + 12)
                    : 240
                // A folder (or anything with no preview) is just its icon: no box around it.
                readonly property bool iconOnly: !insp.preview || insp.preview.children !== undefined
                width: parent.width; height: markdown ? 300 : (isImage ? imageHeight : 150); radius: 2
                color: iconOnly ? "transparent" : Kiki.Theme.bgDark
                border.width: iconOnly ? 0 : 1; border.color: Kiki.Theme.line; clip: true
                Text {
                    visible: insp.preview && insp.preview.text !== undefined && !parent.markdown
                    anchors.fill: parent; anchors.margins: 10
                    text: insp.preview && insp.preview.text !== undefined ? insp.preview.text : ""
                    color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11; lineHeight: 1.4; wrapMode: Text.NoWrap
                }
                // Markdown (plan 23): rendered by Qt's own Markdown support, scrollable, in the UI font.
                Flickable {
                    NaturalScroll { }
                    visible: parent.markdown
                    anchors.fill: parent; anchors.margins: 10; contentHeight: md.height; clip: true; boundsBehavior: Flickable.StopAtBounds
                    Text {
                        id: md
                        width: parent.width
                        textFormat: Text.MarkdownText
                        text: insp.preview && insp.preview.markdown ? insp.preview.text + (insp.preview.truncated ? "\n\n---\n*preview truncated; open the file for the rest*" : "") : ""
                        color: Kiki.Theme.fgDim; linkColor: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: 12; lineHeight: 1.35; wrapMode: Text.WordWrap
                        onLinkActivated: link => Qt.openUrlExternally(link)
                    }
                }
                Image {
                    id: img
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
                // A folder shows its icon rather than a list of what is inside it.
                Icon { visible: !insp.preview || insp.preview.children !== undefined; anchors.centerIn: parent; name: insp.kind(); size: Math.max(48, Math.min(parent.width, parent.height) - 30); strokeWidth: 1; color: Kiki.Theme.kindColor(insp.kind()) }
            }
            Column {
                spacing: 8; width: parent.width
                Field { label: "Type"; value: Kiki.Format.kindLabel(insp.kind()) + (insp.preview && insp.preview.n !== undefined ? " · " + insp.preview.n + (insp.preview.members ? " members" : " items") : "") }
                Field { label: "Host"; value: insp.uri.startsWith("file://") ? "local" : Kiki.Format.authority(insp.uri) }
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
                Button { text: "Open with…"; enabled: insp.row && !insp.row.isDir; onClicked: insp.openWith() }
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
