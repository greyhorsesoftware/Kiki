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
    /// What the panel shows is the row's, and only the row's: the listing is where every value
    /// comes from. (It used to ask the daemon to `Stat` a URI with no meta yet, which on a
    /// server's folder answered NotFound and, on anything, showed nothing the row would not.)
    property var meta: row ? row.meta : null
    /// The narrowest the panel may be dragged: the Permissions grid (88 + 3 × 56) and the revert
    /// mark at its right inside the panel's margins, whole. Both the window's panel and columns
    /// view's info column keep to it.
    readonly property int minWidth: 88 + 3 * 56 + 8 + 24 + 20 + 16
    property bool standalone: false
    /// False where the panel is part of the view rather than something you opened: the info
    /// column in Miller columns follows the selection and has nothing to close to.
    property bool closable: true
    signal closed()
    signal chmod(int mode, bool recursive)
    /// Over a selection: the bits touched (`mask`) and what they were set to (`bits`), for one
    /// job over every item — the daemon merges them into each file's own mode (0.1.1).
    signal chmodMany(int mask, int bits, bool recursive)
    /// The popover's shape (0.1.1): no preview, no grip, tighter header; the card is as tall
    /// as its content (`naturalHeight`).
    property bool compact: false
    /// The selection, when it is more than one row: the panel then shows the selection —
    /// a count, the kinds fanned out, fields summed and merged, a permissions grid that says
    /// where the items differ — instead of the current row alone.
    property var rows: []
    readonly property bool many: rows && rows.length > 1
    function kindsSummary() {
        const counts = {}
        for (const r of rows) { const k = Kiki.Format.kindLabel(r && r.kind ? r.kind : "file"); counts[k] = (counts[k] || 0) + 1 }
        const names = Object.keys(counts).sort((a, b) => counts[b] - counts[a])
        const named = names.slice(0, 2).map(k => Kiki.T.tr("inspector.kindCount", { n: counts[k], kind: k }))
        return names.length > 2 ? Kiki.T.tr("inspector.andMore", { list: named.join(", "), n: names.length - 2 }) : named.join(", ")
    }
    function sizeSummary() {
        let total = 0, measured = 0, unmeasured = 0
        for (const r of rows) { if (r && r.meta && !r.isDir) { total += r.meta.size; measured++ } else unmeasured++ }
        if (!measured) return unmeasured ? "—" : "0 B"
        const sizes = Kiki.T.tr("inspector.bytes", { size: Kiki.Format.bytes(total), n: total })
        return unmeasured ? Kiki.T.tr("inspector.unmeasured", { sizes: sizes, n: unmeasured }) : sizes
    }
    function newestModified() {
        let t = 0
        for (const r of rows) if (r && r.meta && r.meta.mtime > t) t = r.meta.mtime
        return t ? Kiki.T.tr("inspector.newest", { date: Kiki.Format.date(t) }) : "…"
    }
    /// A meta field every item shares, or "mixed".
    function common(field) {
        let v = null
        for (const r of rows) { const x = r && r.meta ? r.meta[field] : undefined; if (x === undefined || x === null || x === "") return "—"; if (v === null) v = x; else if (v !== x) return Kiki.T.tr("inspector.mixedWord") }
        return v === null ? "—" : String(v)
    }
    function gitSummary() {
        const counts = {}
        for (const r of rows) { const st = r && r.git ? r.git.state : "clean"; counts[st] = (counts[st] || 0) + 1 }
        return Object.keys(counts).map(k => Kiki.T.tr("inspector.gitCount", { n: counts[k], state: Kiki.T.tr("git." + k) })).join(", ")
    }
    readonly property bool anyGit: rows.some(r => r && r.git)
    /// The grid over a selection: a bit is on for all, off for all, or mixed; clicking a mixed
    /// or off bit sets it for every item, clicking an on bit clears it for every item, and only
    /// the bits touched go into the job.
    readonly property int allBits: rows.length ? rows.reduce((acc, r) => acc & (r && r.meta && r.meta.mode !== undefined && r.meta.mode !== null ? r.meta.mode & 0o777 : 0o777), 0o777) : 0
    readonly property int anyBits: rows.reduce((acc, r) => acc | (r && r.meta && r.meta.mode !== undefined && r.meta.mode !== null ? r.meta.mode & 0o777 : 0), 0)
    property int touchedMask: 0
    property int touchedBits: 0
    onRowsChanged: { touchedMask = 0; touchedBits = 0 }
    function bitState(bit) {          // "on" | "off" | "mixed", as shown
        if (touchedMask & bit) return (touchedBits & bit) ? "on" : "off"
        if (allBits & bit) return "on"
        return (anyBits & bit) ? "mixed" : "off"
    }
    function toggleBit(bit) {
        const on = bitState(bit) === "on"
        touchedMask |= bit
        touchedBits = on ? (touchedBits & ~bit) : (touchedBits | bit)
    }
    function octalSummary() {
        const modes = []
        for (const r of rows) { if (r && r.meta && r.meta.mode !== undefined && r.meta.mode !== null) { const o = (r.meta.mode & 0o777).toString(8).padStart(3, "0"); if (modes.indexOf(o) < 0) modes.push(o) } }
        if (!modes.length) return "—"
        return modes.length === 1 ? modes[0] : Kiki.T.tr("inspector.mixed", { modes: modes.slice(0, 3).join(", ") + (modes.length > 3 ? ", …" : "") })
    }
    /// What the compact card is tall enough for: header, tabs, the tab's content, the margins.
    readonly property int naturalHeight: body.anchors.topMargin + headerRow.height + body.spacing + tabStrip.height + body.spacing + Math.max(tabLoader.height, otherTab.height) + body.anchors.bottomMargin
    /// Dragging the leading edge: `dx` is the movement, positive to the right.
    signal resized(real dx)
    /// The drag is over, so the width is worth remembering.
    signal resizeEnded()
    /// The grip is held: whoever eases the panel's width must not, while it follows the pointer.
    readonly property bool resizing: grip.pressed

    color: Kiki.Theme.bg
    Rectangle { visible: !standalone; width: 1; height: parent.height; color: Kiki.Theme.line }
    // The panel is as wide as you drag it: a grip on the edge it shares with the listing.
    MouseArea {
        id: grip
        objectName: "inspector-grip"
        visible: !insp.standalone && !insp.compact
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
    /// Mute is the panel's, not one player's: the next video starts the way the last was left.
    property bool videoMuted: false
    function gitLine() {
        const g = row && row.git ? row.git : null
        if (!g) return ""
        const state = Kiki.T.tr("git." + g.state)
        return g.staged ? Kiki.T.tr("git.staged", { state: state }) : state
    }
    function lastCommitLine() {
        const l = gitInfo ? gitInfo.last : null
        return l ? Kiki.T.tr("git.commitLine", { short: l.short, author: l.author, when: Kiki.Format.relative(l.time * 1000) }) : ""
    }
    /// The URI the preview and git state were last asked for. Three inspectors follow the
    /// selection (the docked panel, the card, the columns' info column) and only one is ever
    /// seen: the unseen ones used to ask the daemon for every selection too — Preview and
    /// GitStatus three times a click (2026-09-24). Now an inspector asks when it is seen, and one
    /// that becomes seen with a selection it has not asked about asks then.
    property string _loadedFor: ""
    onUriChanged: { preview = null; gitInfo = null; previewPending = uri !== "" && !many; _loadedFor = ""; if (uri && !many && visible) reload() }
    onVisibleChanged: if (visible && uri && !many && _loadedFor !== uri) reload()
    function reload() {
        if (many) return
        const u = uri
        _loadedFor = u
        Kiki.Daemon.request("Preview", { uri: u }, (ok, err) => { if (u === insp.uri) { preview = ok || null; previewPending = false } })
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
        z: 2; icon: "x"; tip: Kiki.T.tr("inspector.close")
        onClicked: insp.closed()
    }
    Column {
        id: body
        anchors.fill: parent; anchors.margins: insp.compact ? 12 : 16; anchors.leftMargin: insp.compact ? 16 : 20; spacing: insp.compact ? 12 : 16
        // Header: icon, name, path
        Row {
            id: headerRow
            width: parent.width; spacing: 12
            Icon { visible: !insp.many; objectName: "inspector-title-icon"; name: insp.kind(); size: insp.compact ? 24 : 40; strokeWidth: insp.compact ? 1.25 : 1; color: Kiki.Theme.kindColor(insp.kind()) }
            // A selection: the count on a card of its own in the panel; in the card, a little
            // fan of the kinds where the icon would be.
            Rectangle {
                visible: insp.many && !insp.compact
                objectName: "inspector-count"
                width: 40; height: 40; radius: 8; color: Kiki.Theme.surface
                Text { anchors.centerIn: parent; text: insp.rows.length; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 16; font.bold: true }
            }
            KindFan { visible: insp.many && insp.compact; objectName: "inspector-mini-fan"; rows: insp.rows; cardWidth: 24; most: 3; width: 56; height: 36; anchors.verticalCenter: parent.verticalCenter }
            Column {
                objectName: "inspector-title"
                // Level with the icon beside it, not hung from the top of the row.
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - (insp.compact ? (insp.many ? 68 : 36) : 52); spacing: 4
                // The name alone: the folder it is in is a field under General, and repeating the
                // whole path here only crowded the header.
                Text { width: parent.width; elide: Text.ElideRight; text: insp.many ? Kiki.T.tr("inspector.items", { n: insp.rows.length }) : insp.name(); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: insp.compact ? 13 : 15; font.bold: true }
            }
        }
        // The preview is of the file, not of a tab: it stays while the tabs change under it.
        Rectangle {
            id: previewBox
            objectName: "inspector-preview"
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
            // The picture's height over its width, SET when it has loaded rather than bound to its
            // implicit size: the picture fills this box, so a height bound to the picture is bound
            // to itself ("Binding loop detected for property height", on every image).
            property real ratio: 0
            readonly property int imageHeight: ratio > 0 ? Math.min(400, Math.round((width - 12) * ratio) + 12) : 240
            // A folder (or anything with no preview) is just its icon: no box around it.
            readonly property bool iconOnly: !isImage && (!insp.preview || insp.preview.children !== undefined)
            width: parent.width
            visible: !insp.compact
            height: insp.compact ? 0 : (insp.many ? 112 : Math.min(markdown ? 300 : (isImage ? imageHeight : 150), Math.round(insp.height * 0.4)))
            radius: 2
            // A picture is its own frame; the box is for text, where an edge helps. A selection's
            // fan has no box at all.
            color: (iconOnly || isImage || insp.many) ? "transparent" : Kiki.Theme.bgDark
            border.width: (iconOnly || isImage || insp.many) ? 0 : 1; border.color: Kiki.Theme.line; clip: true
            KindFan { visible: insp.many; objectName: "inspector-fan"; rows: insp.rows; cardWidth: 64; anchors.fill: parent }
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
                onStatusChanged: previewBox.ratio = (status === Image.Ready && implicitWidth > 0) ? implicitHeight / implicitWidth : 0
                onSourceChanged: if (source == "") previewBox.ratio = 0
            }
            Loader {
                id: video
                objectName: "inspector-video"
                anchors.fill: parent; anchors.margins: 6
                active: previewBox.videoOn
                source: "VideoPreview.qml"
                onLoaded: item.source = insp.uri
            }
            Binding { target: video.item; property: "muted"; value: insp.videoMuted; when: video.item !== null }
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
                readonly property bool hovered: containsMouse || playArea.containsMouse || openArea.containsMouse || muteArea.containsMouse || scrubArea.containsMouse || scrubArea.pressed
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
                        Tip { visible: openArea.containsMouse; text: Kiki.T.tr("inspector.open") }
                        MouseArea {
                            id: openArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                            // Two players talking over each other helps nobody: ours stops.
                            onClicked: { if (video.item && video.item.playing) video.item.toggle(); insp.open(insp.uri) }
                        }
                    }
                }
            }
            function toggleVideo() { if (!videoOn) videoOn = true; else if (video.item) video.item.toggle() }
            // Under the picture, once it is a player: sound, where it is, and how long it is. A
            // preview nobody can silence or wind on is a bad neighbour (plan 29 F).
            Rectangle {
                objectName: "inspector-video-strip"
                visible: previewBox.videoOn && !!video.item && (videoHover.hovered || !video.item.playing)
                anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.margins: 6
                height: 28; color: Qt.rgba(0, 0, 0, 0.6)
                Row {
                    anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 8; spacing: 8
                    Item {
                        objectName: "inspector-video-mute"
                        width: 22; height: parent.height
                        Icon { anchors.centerIn: parent; name: insp.videoMuted ? "volume-off" : "volume"; size: 14; color: video.item && !video.item.hasAudio ? Qt.rgba(1, 1, 1, 0.35) : "white" }
                        Tip { visible: muteArea.containsMouse; text: video.item && !video.item.hasAudio ? "No sound in this video" : (insp.videoMuted ? "Unmute" : "Mute") }
                        // A click soon after the one that started the video arrives as the second half of a
                        // double click, which a MouseArea reports instead of `clicked`: it counts too.
                        MouseArea { id: muteArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: insp.videoMuted = !insp.videoMuted; onDoubleClicked: insp.videoMuted = !insp.videoMuted }
                    }
                    Item {
                        objectName: "inspector-video-scrub"
                        width: parent.width - 22 - clockText.width - 2 * parent.spacing; height: parent.height
                        readonly property real fraction: video.item && video.item.duration > 0 ? video.item.position / video.item.duration : 0
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: 3; radius: 1.5; color: Qt.rgba(1, 1, 1, 0.25) }
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Math.round(parent.width * parent.fraction); height: 3; radius: 1.5; color: Kiki.Theme.accent }
                        Rectangle { visible: scrubArea.containsMouse || scrubArea.pressed; anchors.verticalCenter: parent.verticalCenter; x: Math.round(parent.width * parent.fraction) - 5; width: 10; height: 10; radius: 5; color: "white" }
                        MouseArea {
                            id: scrubArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; preventStealing: true
                            function go(x) { if (video.item && video.item.duration > 0) video.item.seek(video.item.duration * Math.max(0, Math.min(1, x / width))) }
                            onPressed: mouse => go(mouse.x)
                            onPositionChanged: mouse => { if (pressed) go(mouse.x) }
                        }
                    }
                    Text {
                        id: clockText
                        objectName: "inspector-video-clock"
                        anchors.verticalCenter: parent.verticalCenter
                        text: video.item ? Kiki.Format.clock(video.item.position) + " / " + Kiki.Format.clock(video.item.duration) : ""
                        color: "white"; font.family: Kiki.Theme.mono; font.pixelSize: 10
                    }
                }
            }
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
                visible: !insp.many && (insp.previewPending ? !!(insp.row && insp.row.isDir) : previewBox.iconOnly)
                anchors.centerIn: parent; name: insp.kind(); size: Math.max(48, Math.min(parent.width, parent.height) - 30); strokeWidth: 1; color: Kiki.Theme.kindColor(insp.kind())
            }
        }
        // Tabs
        Item {
            id: tabStrip
            width: parent.width; height: 32
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
            Row {
                spacing: 20; height: parent.height
                Repeater {
                    model: [{ id: "general", label: Kiki.T.tr("inspector.general") }, { id: "permissions", label: Kiki.T.tr("inspector.permissions") }]
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
            width: parent.width; height: insp.compact ? tabLoader.height : Math.max(0, parent.height - previewBox.height - 120)
            contentWidth: width; contentHeight: tabLoader.height
            clip: true; boundsBehavior: Flickable.StopAtBounds
            NaturalScroll { }
            Loader { id: tabLoader; width: parent.width; sourceComponent: insp.tab === "general" ? general : permissions }
            // The other tab, laid out but not shown, so the compact card can be as tall as the
            // taller of the two and not resize when the tabs change (owner, 2026-09-24).
            // The other tab, laid out but not seen, so the card is as tall as the taller of the two
            // and does not resize on a tab switch. Unseen by opacity, not `visible: false`: in a
            // hidden subtree a row's `visible` change never reaches its Column, so the copy kept
            // the layout it was created with (the single-file Symbolic row over a selection, a
            // git-tracked file's rows over a plain one) and the card opened too tall, shrinking
            // when the tab was switched and the copy remade (2026-09-24).
            Loader { id: otherTab; objectName: "other-tab"; opacity: 0; enabled: false; active: insp.compact; width: parent.width; sourceComponent: insp.tab === "general" ? permissions : general }
        }
    }
    Item {
    }

    function ownerOf() { return insp.many ? insp.common("owner") : (insp.meta && insp.meta.owner ? insp.meta.owner : "—") }
    function groupOf() { return insp.many ? insp.common("group") : (insp.meta && insp.meta.group ? insp.meta.group : "—") }
    /// The card's one line for both (owner, 2026-09-24: "shrink it height-wise").
    function ownerGroup() { return ownerOf() + " · " + groupOf() }

    /// The label column: as wide as the longest label in the language, never narrower than the
    /// 88 px English fits in ("Propietario/Grupo" ran into its value, 2026-09-25).
    property FontMetrics labelFont: FontMetrics { font.family: Kiki.Theme.mono; font.pixelSize: 12 }
    readonly property int labelCol: {
        const keys = ["inspector.ownerGroup", "inspector.lastCommit", "inspector.location", "inspector.modified", "inspector.symbolic", "inspector.branch"]
        let w = 88
        for (const k of keys) w = Math.max(w, Math.ceil(labelFont.advanceWidth(Kiki.T.tr(k))) + 6)
        return Kiki.T.language ? w : w         // re-read when the language changes
    }
    component Field: Row {
        property string label: ""
        property string value: ""
        property color valueColor: Kiki.Theme.fg
        spacing: 12; width: parent.width
        Text { width: insp.labelCol; text: label; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Text { width: parent.width - insp.labelCol - 12; wrapMode: Text.WrapAnywhere; text: value; color: valueColor; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
    }

    Component {
        id: general
        Column {
            spacing: 18
            Column {
                spacing: 8; width: parent.width
                Field { objectName: "insp-type"; label: insp.many ? Kiki.T.tr("inspector.kinds") : Kiki.T.tr("inspector.type"); value: insp.many ? insp.kindsSummary() : Kiki.Format.kindLabel(insp.kind()) + (insp.preview && insp.preview.n !== undefined ? " · " + insp.preview.n + (insp.preview.members ? " members" : " items") : "") }
                Field { label: Kiki.T.tr("inspector.host"); value: insp.uri.startsWith("file://") ? Kiki.T.tr("inspector.local") : Kiki.Format.authority(insp.uri) }
                Field { label: Kiki.T.tr("inspector.location"); value: Kiki.Format.display(insp.uri.slice(0, insp.uri.lastIndexOf("/")) || insp.uri, insp.home) }
            }
            Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
            Column {
                spacing: 8; width: parent.width
                Field { objectName: "insp-size"; label: Kiki.T.tr("inspector.size"); value: insp.many ? insp.sizeSummary() : (insp.meta ? (insp.row && insp.row.isDir ? "—" : Kiki.Format.bytes(insp.meta.size) + " (" + insp.meta.size.toLocaleString(Qt.locale(), "f", 0) + " bytes)") : "…") }
                Field { objectName: "insp-modified"; label: Kiki.T.tr("inspector.modified"); value: insp.many ? insp.newestModified() : (insp.meta ? Kiki.Format.date(insp.meta.mtime) : "…") }
                Field { objectName: "insp-owner"; label: insp.compact ? Kiki.T.tr("inspector.ownerGroup") : Kiki.T.tr("inspector.owner"); value: insp.compact ? insp.ownerGroup() : insp.ownerOf() }
                Field { objectName: "insp-group"; label: Kiki.T.tr("inspector.group"); visible: !insp.compact; value: insp.groupOf() }
                Field { objectName: "insp-git"; label: Kiki.T.tr("inspector.git"); value: insp.many ? insp.gitSummary() : (insp.gitLine() || "—"); valueColor: !insp.many && insp.row && insp.row.git ? Kiki.Format.gitColor(insp.row.git) : Kiki.Theme.fg; visible: insp.many ? insp.anyGit : !!(insp.row && insp.row.git) }
                Field { objectName: "insp-git-branch"; label: Kiki.T.tr("inspector.branch"); value: insp.gitInfo && insp.gitInfo.branch ? insp.gitInfo.branch : ""; visible: value !== "" }
                Field { objectName: "insp-git-last"; label: Kiki.T.tr("inspector.lastCommit"); value: insp.lastCommitLine(); visible: value !== "" }
                Field { objectName: "insp-git-subject"; label: ""; value: insp.gitInfo && insp.gitInfo.last ? insp.gitInfo.last.subject : ""; valueColor: Kiki.Theme.fgDim; visible: value !== "" }
            }
        }
    }

    Component {
        id: permissions
        Column {
            spacing: insp.compact ? 10 : 14
            property bool canEdit: insp.many ? insp.rows.some(r => r && r.meta && r.meta.mode !== null && r.meta.mode !== undefined) : (insp.meta && insp.meta.mode !== null && insp.meta.mode !== undefined)
            /// Something changed: the revert mark at the grid's right lights.
            readonly property bool changed: insp.many ? insp.touchedMask !== 0 : insp.dirty
            Item {
            width: parent.width; height: grid.height
            Grid {
                id: grid
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
                                // One file: the bit as edited. A selection: on, off, or mixed across it.
                                property string state: insp.many ? insp.bitState(bit << who.shift) : (((insp.editMode >> who.shift) & bit) ? "on" : "off")
                                property bool on: state === "on"
                                anchors.centerIn: parent; width: 16; height: 16; radius: 2
                                color: on ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: on ? Kiki.Theme.accent : Kiki.Theme.gutter
                                Icon { visible: parent.on; anchors.centerIn: parent; name: "check"; size: 10; strokeWidth: 2.5; color: Kiki.Theme.bg }
                                // The dash: this bit differs across the selection.
                                Rectangle { visible: parent.state === "mixed"; anchors.centerIn: parent; width: 8; height: 2; color: Kiki.Theme.fgDim }
                                MouseArea { anchors.fill: parent; onClicked: { if (!canEdit) return; if (insp.many) insp.toggleBit(parent.bit << who.shift); else insp.editMode ^= (parent.bit << who.shift) } }
                            }
                        }
                    }
                }
            }
            // Revert, as a mark to the grid's right rather than a button (owner, 2026-09-24): dim
            // until something is changed, lit once it is; a click puts the grid back to what it
            // showed when the panel opened — one file's mode as loaded, a selection's own bits —
            // and sends nothing.
            Item {
                objectName: "perm-revert"
                readonly property bool lit: parent.parent.changed
                enabled: lit
                width: 24; height: 24; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                Icon { anchors.centerIn: parent; name: "undo"; size: 16; color: parent.lit ? Kiki.Theme.accent : Kiki.Theme.gutter }
                Tip { visible: parent.lit && revertHover.containsMouse; text: Kiki.T.tr("inspector.revert") }
                MouseArea { id: revertHover; anchors.fill: parent; hoverEnabled: true; cursorShape: parent.lit ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: if (insp.many) { insp.touchedMask = 0; insp.touchedBits = 0 } else insp.editMode = insp.meta.mode & 0o777 }
            }
            }
            Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
            Column {
                spacing: 8; width: parent.width
                Field { objectName: "perm-octal"; label: Kiki.T.tr("inspector.octal"); value: insp.many ? insp.octalSummary() : (canEdit ? insp.editMode.toString(8).padStart(3, "0") : "—") }
                Field { label: Kiki.T.tr("inspector.symbolic"); visible: !insp.many; value: canEdit ? insp.symbolic(insp.editMode) : "—" }
                Field { objectName: "perm-owner"; label: insp.compact ? Kiki.T.tr("inspector.ownerGroup") : Kiki.T.tr("inspector.owner"); value: insp.compact ? insp.ownerGroup() : insp.ownerOf() }
                Field { objectName: "perm-group"; label: Kiki.T.tr("inspector.group"); visible: !insp.compact; value: insp.groupOf() }
            }
            // Centred, with Apply under it, in the card and the panel alike (owner, 2026-09-24).
            Row {
                id: recursiveRow
                spacing: 8; anchors.horizontalCenter: parent.horizontalCenter
                property bool recursive: false
                Rectangle {
                    objectName: "perm-recursive"
                    width: 16; height: 16; radius: 2; anchors.verticalCenter: parent.verticalCenter
                    color: recursiveRow.recursive ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: recursiveRow.recursive ? Kiki.Theme.accent : Kiki.Theme.gutter
                    MouseArea { anchors.fill: parent; onClicked: recursiveRow.recursive = !recursiveRow.recursive }
                }
                Text { anchors.verticalCenter: parent.verticalCenter; text: Kiki.T.tr("inspector.recursive"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            }
            Item { width: 1; height: insp.compact ? 0 : 8 }
            Row {
                spacing: 8; anchors.horizontalCenter: parent.horizontalCenter
                Button { objectName: "perm-apply"; text: Kiki.T.tr("inspector.apply"); primary: true; small: insp.compact; enabled: insp.many ? insp.touchedMask !== 0 : insp.dirty
                         onClicked: if (insp.many) insp.chmodMany(insp.touchedMask, insp.touchedBits, recursiveRow.recursive); else insp.chmod(insp.editMode, recursiveRow.recursive) }
            }
        }
    }
}
