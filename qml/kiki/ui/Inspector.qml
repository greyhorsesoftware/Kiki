import QtQuick
import QtQuick.Window
import ".." as Kiki

// Tabbed inspector: General (preview and fields) and Permissions. Reads through
// the daemon's Preview and Stat; chmod applies as a plan-04 job.
Rectangle {
    id: insp
    property string uri: ""
    property var row: null           // the listing row when known (name, kind, meta)
    property string tab: "general"   // general | permissions
    signal edit(string uri, int line)
    /// Open this file in the application that owns its type.
    signal open(string uri)
    property string home: ""
    property var preview: null
    property var meta: row ? row.meta : null
    property bool standalone: false
    /// False where the panel is part of the view rather than something you opened: the info
    /// column in Miller columns follows the selection and has nothing to close to.
    property bool closable: true
    signal closed()
    signal chmod(int mode, bool recursive)
    /// Dragging the leading edge: `dx` is the movement, positive to the right.
    signal resized(real dx)
    /// The drag is over, so the width is worth remembering.
    signal resizeEnded()

    color: Kiki.Theme.bg
    Rectangle { visible: !standalone; width: 1; height: parent.height; color: Kiki.Theme.line }
    // The panel is as wide as you drag it: a grip on the edge it shares with the listing.
    MouseArea {
        id: grip
        objectName: "inspector-grip"
        visible: !insp.standalone
        width: 6; height: parent.height; z: 20
        cursorShape: Qt.SplitHCursor
        hoverEnabled: true
        preventStealing: true
        property real last: 0
        onPressed: mouse => last = mouse.x
        onPositionChanged: mouse => { if (pressed) { insp.resized(mouse.x - last) } }
        onReleased: insp.resizeEnded()
        Rectangle { anchors.fill: parent; color: grip.containsMouse || grip.pressed ? Kiki.Theme.accent : "transparent"; opacity: 0.5 }
    }

    /// The daemon has been asked what this file looks like and has not said yet. Until it does
    /// the preview box stays empty: "no preview" is an answer, and drawing the kind icon before
    /// it arrives meant every file showed its icon for a moment and then its preview.
    property bool previewPending: false
    /// Git's detail for the file (plan 15): its branch and last commit. Asked for only when the
    /// row says it is in a repository, since it costs a `git log`; the state itself is the row's.
    property var gitInfo: null
    function gitLine() {
        const g = row && row.git ? row.git : null
        if (!g) return ""
        return g.state + (g.staged ? " · staged" : "")
    }
    function lastCommitLine() {
        const l = gitInfo ? gitInfo.last : null
        return l ? l.short + " · " + l.author + " · " + Kiki.Format.relative(l.time * 1000) : ""
    }
    onUriChanged: { preview = null; gitInfo = null; previewPending = uri !== ""; if (uri) reload() }
    function reload() {
        const u = uri
        Kiki.Daemon.request("Preview", { uri: u }, (ok, err) => { if (u === insp.uri) { preview = ok || null; previewPending = false } })
        if (!meta) Kiki.Daemon.request("Stat", { uri: u }, (ok, err) => { if (u === insp.uri && ok) insp.meta = ok })
        if (row && row.git) Kiki.Daemon.request("GitStatus", { uri: u }, (ok, err) => { if (u === insp.uri) insp.gitInfo = ok || null })
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

    // A panel you asked for needs a visible way out; Ctrl+I toggles it too.
    ToggleButton {
        anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 6
        objectName: "inspector-close"
        visible: insp.closable
        z: 2; icon: "x"; tip: "Close (Ctrl+I)"
        onClicked: insp.closed()
    }
    Column {
        anchors.fill: parent; anchors.margins: 16; anchors.leftMargin: 20; spacing: 16
        // Header: icon, name, path
        Row {
            width: parent.width; spacing: 12
            Icon { objectName: "inspector-title-icon"; name: insp.kind(); size: 40; strokeWidth: 1; color: Kiki.Theme.kindColor(insp.kind()) }
            Column {
                objectName: "inspector-title"
                // Level with the icon beside it, not hung from the top of the row.
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 52; spacing: 4
                // The name alone: the folder it is in is a field under General, and repeating the
                // whole path here only crowded the header.
                Text { width: parent.width; elide: Text.ElideRight; text: insp.name(); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            }
        }
        // The preview is of the file, not of a tab: it stays while the tabs change under it.
        Rectangle {
            id: previewBox
            readonly property bool markdown: insp.preview && insp.preview.markdown === true
            // A local picture needs no answer from the daemon to start drawing: it is the file.
            readonly property bool isImage: localImage || (insp.preview && insp.preview.path !== undefined)
            /// A local picture is read from the file itself rather than from its 256px thumbnail,
            /// which the panel is wide enough to show blown up and blurred.
            readonly property bool localImage: insp.kind() === "image" && insp.uri.indexOf("file://") === 0
            /// A video on this machine can be played where its still is. Only on a click: a
            /// selection moving down a folder must not start ten players.
            readonly property bool localVideo: insp.kind() === "video" && insp.uri.indexOf("file://") === 0
            property bool videoOn: false
            Connections { target: insp; function onUriChanged() { previewBox.videoOn = false } }
            // An image preview takes the width of the panel and keeps its own ratio, rather
            // than sitting letterboxed in a short box.
            readonly property int imageHeight: img.implicitWidth > 0
                ? Math.min(400, Math.round((width - 12) * img.implicitHeight / img.implicitWidth) + 12)
                : 240
            // A folder (or anything with no preview) is just its icon: no box around it.
            readonly property bool iconOnly: !isImage && (!insp.preview || insp.preview.children !== undefined)
            width: parent.width
            height: Math.min(markdown ? 300 : (isImage ? imageHeight : 150), Math.round(insp.height * 0.4))
            radius: 2
            // A picture is its own frame; the box is for text, where an edge helps.
            color: (iconOnly || isImage) ? "transparent" : Kiki.Theme.bgDark
            border.width: (iconOnly || isImage) ? 0 : 1; border.color: Kiki.Theme.line; clip: true
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
                visible: previewBox.isImage
                anchors.fill: parent; anchors.margins: 6; fillMode: Image.PreserveAspectFit
                asynchronous: true; smooth: true; mipmap: true; cache: false
                source: previewBox.localImage ? insp.uri : (insp.preview && insp.preview.path ? "file://" + insp.preview.path : "")
                // Decoded at the size it is drawn at, on this screen: anything less shows.
                sourceSize: Qt.size(Math.round(previewBox.width * Screen.devicePixelRatio),
                                    Math.round(400 * Screen.devicePixelRatio))
            }
            Loader {
                id: video
                objectName: "inspector-video"
                anchors.fill: parent; anchors.margins: 6
                active: previewBox.videoOn
                source: "VideoPreview.qml"
                onLoaded: item.source = insp.uri
            }
            // The still is the play button; the one floating over its middle shows under the
            // pointer and pauses as well. Below the panel's own close box and resize grip.
            MouseArea {
                id: videoHover
                objectName: "inspector-video-area"
                visible: previewBox.localVideo && previewBox.isImage
                anchors.fill: parent; hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: previewBox.toggleVideo()
                // Two round buttons over the middle of the picture: play/pause, and open it in
                // the app that owns the type. `hovered` spans the buttons too — a MouseArea
                // inside another takes the pointer from it, and the pair would blink out.
                readonly property bool hovered: containsMouse || playArea.containsMouse || openArea.containsMouse
                Row {
                    objectName: "inspector-video-buttons"
                    visible: videoHover.hovered
                    anchors.centerIn: parent; spacing: 14
                    Rectangle {
                        objectName: "inspector-video-button"
                        width: 48; height: 48; radius: 24
                        color: Qt.rgba(0, 0, 0, playArea.containsMouse ? 0.75 : 0.55); border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.25)
                        Icon { anchors.centerIn: parent; name: video.item && video.item.playing ? "pause" : "play"; size: 20; color: "white" }
                        MouseArea { id: playArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: previewBox.toggleVideo() }
                    }
                    Rectangle {
                        objectName: "inspector-video-open"
                        width: 48; height: 48; radius: 24
                        color: Qt.rgba(0, 0, 0, openArea.containsMouse ? 0.75 : 0.55); border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.25)
                        Icon { anchors.centerIn: parent; name: "open"; size: 20; color: "white" }
                        Tip { visible: openArea.containsMouse; text: "Open" }
                        MouseArea {
                            id: openArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                            // Two players talking over each other helps nobody: ours stops.
                            onClicked: { if (video.item && video.item.playing) video.item.toggle(); insp.open(insp.uri) }
                        }
                    }
                }
            }
            function toggleVideo() { if (!videoOn) videoOn = true; else if (video.item) video.item.toggle() }
            Column {
                visible: insp.preview && insp.preview.members !== undefined
                anchors.fill: parent; anchors.margins: 10
                Repeater { model: insp.preview && insp.preview.members ? insp.preview.members.slice(0, 9) : []; delegate: Text { required property var modelData; text: (modelData.isDir ? "" : "  ") + modelData.name; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11; elide: Text.ElideMiddle; width: 250 } }
            }
            // A folder shows its icon rather than a list of what is inside it.
            // Only once that is known: a folder says so in its row, anything else when the
            // daemon has answered.
            Icon {
                objectName: "inspector-kind-icon"
                visible: insp.previewPending ? !!(insp.row && insp.row.isDir) : previewBox.iconOnly
                anchors.centerIn: parent; name: insp.kind(); size: Math.max(48, Math.min(parent.width, parent.height) - 30); strokeWidth: 1; color: Kiki.Theme.kindColor(insp.kind())
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
                        objectName: "tab-" + modelData.id
                        width: t.implicitWidth + 4; height: 32
                        Text { id: t; anchors.centerIn: parent; text: modelData.label; color: insp.tab === modelData.id ? Kiki.Theme.fg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: insp.tab === modelData.id ? Kiki.Theme.accent : "transparent" }
                        MouseArea { anchors.fill: parent; onClicked: insp.tab = modelData.id }
                    }
                }
            }
        }
        Flickable {
            // Whatever the header, the preview and the tab strip leave.
            width: parent.width; height: Math.max(0, parent.height - previewBox.height - 120)
            contentWidth: width; contentHeight: tabLoader.height
            clip: true; boundsBehavior: Flickable.StopAtBounds
            NaturalScroll { }
            Loader { id: tabLoader; width: parent.width; sourceComponent: insp.tab === "general" ? general : permissions }
        }
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
                Field { objectName: "insp-git"; label: "Git"; value: insp.gitLine() || "—"; valueColor: insp.row && insp.row.git ? Kiki.Format.gitColor(insp.row.git) : Kiki.Theme.fg; visible: !!(insp.row && insp.row.git) }
                Field { objectName: "insp-git-branch"; label: "Branch"; value: insp.gitInfo && insp.gitInfo.branch ? insp.gitInfo.branch : ""; visible: value !== "" }
                Field { objectName: "insp-git-last"; label: "Last commit"; value: insp.lastCommitLine(); visible: value !== "" }
                Field { objectName: "insp-git-subject"; label: ""; value: insp.gitInfo && insp.gitInfo.last ? insp.gitInfo.last.subject : ""; valueColor: Kiki.Theme.fgDim; visible: value !== "" }
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
                                objectName: "perm-" + who.who.toLowerCase() + "-" + bit
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
                id: recursiveRow
                spacing: 8
                property bool recursive: false
                Rectangle {
                    objectName: "perm-recursive"
                    width: 16; height: 16; radius: 2; anchors.verticalCenter: parent.verticalCenter
                    color: recursiveRow.recursive ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: recursiveRow.recursive ? Kiki.Theme.accent : Kiki.Theme.gutter
                    MouseArea { anchors.fill: parent; onClicked: recursiveRow.recursive = !recursiveRow.recursive }
                }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "Apply to contained items"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            }
            Item { width: 1; height: 8 }
            Row {
                spacing: 8
                Button { objectName: "perm-apply"; text: "Apply"; primary: true; enabled: insp.dirty; onClicked: insp.chmod(insp.editMode, recursiveRow.recursive) }
                Button { objectName: "perm-revert"; text: "Revert"; enabled: insp.dirty; onClicked: insp.editMode = insp.meta.mode & 0o777 }
            }
        }
    }
}
