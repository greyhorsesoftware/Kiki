import QtQuick
import ".." as Kiki
import "../ui" as UI

// Gallery (plan 27): one picture filling the pane over a filmstrip of its neighbours. The
// listing, its order and the selection are the other views' — only the drawing differs.
Item {
    id: root
    property Kiki.Pane pane
    property string home: ""
    signal activate(int index)
    signal contextMenu(int index, point pos)

    property bool filmstrip: true
    property real zoom: 0                       // 0 = fit the stage
    readonly property int stripHeight: filmstrip ? 84 : 0
    readonly property int current: pane ? pane.selection.current : -1
    property var row: null
    function refreshRow() { row = (pane && current >= 0) ? pane.listing.row(current) : null }
    onCurrentChanged: refreshRow()
    readonly property bool isImage: row && (row.kind === "image" || row.kind === "video")
    readonly property string uri: row && pane ? pane.childUri(row.name) : ""
    // Remote files cannot be handed to the loader as a path; their cached thumbnail stands in.
    readonly property string source: !row ? "" : (uri.startsWith("file://") ? uri : (row.thumb ? "file://" + row.thumb : ""))

    /// Opening a folder clears the selection, which would leave the stage on a placeholder.
    /// Land on the first picture instead, or the first row when none has arrived yet.
    function selectFirst() {
        if (!pane || current >= 0 || pane.listing.count === 0) return
        for (let i = 0; i < Math.min(pane.listing.count, 200); i++) {
            const r = pane.listing.row(i)
            if (r && (r.kind === "image" || r.kind === "video")) { pane.selection.set(i); ensureVisible(i); return }
        }
        pane.selection.set(0)
    }
    Component.onCompleted: { selectFirst(); refreshRow() }
    Connections {
        target: root.pane ? root.pane.listing : null
        function onCountChanged() { root.selectFirst(); root.refreshRow() }
        function onReset() { root.selectFirst(); root.refreshRow() }
        function onRowsUpdated(first, n) { root.selectFirst(); root.refreshRow() }
    }

    // The shell drives every view through these.
    function ensureVisible(i) { strip.positionViewAtIndex(i, ListView.Contain) }
    readonly property int perRow: 1
    readonly property int pageSize: 1
    function step(d) {
        const n = pane.listing.count; if (!n) return
        const i = Math.max(0, Math.min(n - 1, (current < 0 ? 0 : current + d)))
        pane.selection.set(i)
        ensureVisible(i)
        zoom = 0
    }
    function fit() { zoom = 0 }
    function actual() { zoom = 1 }
    function zoomBy(f) { zoom = Math.max(0.1, Math.min(8, (zoom || fitScale) * f)) }
    readonly property real fitScale: img.implicitWidth > 0
        ? Math.min(1, Math.min(stage.width / img.implicitWidth, stage.height / img.implicitHeight))
        : 1

    Flickable {
        id: stage
        width: parent.width; height: parent.height - root.stripHeight
        contentWidth: Math.max(width, img.width); contentHeight: Math.max(height, img.height)
        clip: true; boundsBehavior: Flickable.StopAtBounds
        Image {
            id: img
            visible: root.isImage && root.source !== ""
            source: root.source
            asynchronous: true; smooth: true; cache: false
            fillMode: Image.PreserveAspectFit
            // Decoded at the size it is shown at, not the size it was taken at.
            sourceSize: Qt.size(Math.max(64, stage.width * 2), Math.max(64, stage.height * 2))
            width: root.zoom === 0 ? Math.round(implicitWidth * root.fitScale) : Math.round(implicitWidth * root.zoom)
            height: root.zoom === 0 ? Math.round(implicitHeight * root.fitScale) : Math.round(implicitHeight * root.zoom)
            x: Math.max(0, (stage.contentWidth - width) / 2)
            y: Math.max(0, (stage.contentHeight - height) / 2)
        }
        // Anything that is not a picture, or has not loaded, keeps its kind icon.
        Column {
            visible: !img.visible || img.status !== Image.Ready
            anchors.centerIn: parent; spacing: 12
            UI.Icon {
                anchors.horizontalCenter: parent.horizontalCenter
                name: root.row ? root.row.kind : "image"; size: 96; strokeWidth: 1
                color: Kiki.Theme.kindColor(root.row ? root.row.kind : "image")
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: !root.pane || root.pane.listing.count === 0 ? "No pictures here"
                    : (root.row ? (img.status === Image.Loading ? "loading…" : root.row.name) : "")
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
        }
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onDoubleClicked: root.activate(root.current)
            onClicked: mouse => { if (mouse.button === Qt.RightButton) root.contextMenu(root.current, root.mapToItem(null, mouse.x, mouse.y)) }
        }
        UI.NaturalScroll { }
    }

    Rectangle {
        id: stripBar
        visible: root.filmstrip
        y: stage.height; width: parent.width; height: root.stripHeight
        color: Kiki.Theme.bgDark
        Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
        ListView {
            id: strip
            anchors.fill: parent; anchors.topMargin: 6; anchors.bottomMargin: 6; anchors.leftMargin: 8
            orientation: ListView.Horizontal; spacing: 6
            clip: true; reuseItems: true
            model: root.pane ? root.pane.listing.count : 0
            onContentXChanged: root.pane.listing.setViewport(Math.max(0, Math.floor(contentX / 78)), Math.ceil(width / 78) + 2)
            Connections { target: root.pane ? root.pane.listing : null; function onReset() { strip.forceLayout() } }
            delegate: Rectangle {
                id: shot
                required property int index
                property var r: root.pane.listing.row(index)
                width: 72; height: strip.height; radius: 2
                color: "transparent"
                border.width: index === root.current ? 2 : 0
                border.color: Kiki.Theme.accent
                Connections { target: root.pane.listing; function onRowsUpdated(first, n) { if (shot.index >= first && shot.index < first + n) shot.r = root.pane.listing.row(shot.index) } }
                Image {
                    visible: shot.r && shot.r.thumb
                    anchors.fill: parent; anchors.margins: 3
                    source: shot.r && shot.r.thumb ? "file://" + shot.r.thumb : ""
                    sourceSize: Qt.size(144, 144); fillMode: Image.PreserveAspectFit
                    asynchronous: true; smooth: true
                }
                UI.Icon {
                    visible: !(shot.r && shot.r.thumb)
                    anchors.centerIn: parent; size: 28
                    name: shot.r ? shot.r.kind : "file"; color: Kiki.Theme.kindColor(shot.r ? shot.r.kind : "file")
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: { root.pane.selection.set(shot.index); root.zoom = 0 }
                    onDoubleClicked: root.activate(shot.index)
                }
            }
            // Two-finger sideways swipe walks the strip, as in columns view.
            WheelHandler {
                target: null
                orientation: Qt.Horizontal
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                onWheel: event => {
                    const dx = event.pixelDelta.x !== 0 ? event.pixelDelta.x : event.angleDelta.x / 2
                    if (dx === 0) { event.accepted = false; return }
                    strip.contentX = Math.max(0, Math.min(strip.contentX + dx, Math.max(0, strip.contentWidth - strip.width)))
                    event.accepted = true
                }
            }
        }
    }
}
