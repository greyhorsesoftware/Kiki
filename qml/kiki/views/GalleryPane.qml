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
    /// The bar under the picture: play, pause and how the slideshow runs.
    readonly property int controlsHeight: 40
    /// What the picture has to itself.
    readonly property int stageHeight: Math.max(0, height - stripHeight - controlsHeight)

    // ---------------------------------------------------------------- slideshow
    property bool playing: false
    readonly property int slideDelay: Math.max(1, Kiki.Settings.view.slideshowDelay || 4)
    readonly property bool slideLoop: Kiki.Settings.view.slideshowLoop !== false
    function togglePlay() { playing = !playing }
    function advance() {
        if (!pane) return
        // step() says false at the last picture; from there it either starts again or stops.
        if (root.step(1)) return
        if (!slideLoop) { playing = false; return }
        for (let i = 0; i < pane.listing.count; i++) {
            const r = pane.listing.row(i)
            if (r && (r.kind === "image" || r.kind === "video")) { pane.selection.set(i); root.zoom = 0; return }
        }
        playing = false
    }
    property Timer slideshow: Timer {
        interval: root.slideDelay * 1000
        repeat: true
        running: root.playing && root.pane && root.pane.listing.count > 1
        onTriggered: root.advance()
    }
    /// One thumbnail plus the gap after it: the layout and the viewport maths share it, or the
    /// daemon is told about the wrong rows and the thumbnails at the edges never arrive.
    readonly property int shotWidth: 72
    readonly property int shotGap: 8
    readonly property int shotPitch: shotWidth + shotGap
    readonly property int current: pane ? pane.selection.current : -1
    property var row: null
    function refreshRow() { row = (pane && current >= 0) ? pane.listing.row(current) : null }
    onCurrentChanged: refreshRow()
    readonly property bool isImage: row && (row.kind === "image" || row.kind === "video")
    readonly property string uri: row && pane ? pane.childUri(row.name) : ""
    // Remote files cannot be handed to the loader as a path; their cached thumbnail stands in.
    readonly property string source: !row ? "" : (uri.startsWith("file://") ? uri : (row.thumb ? "file://" + row.thumb : ""))

    /// Tell the daemon which rows the filmstrip is showing, so their thumbnails get made.
    function syncStrip() {
        if (!pane) return
        pane.listing.setViewport(Math.max(0, Math.floor(strip.contentX / shotPitch)), Math.ceil(strip.width / shotPitch) + 2)
    }
    /// Opening a folder clears the selection, which would leave the stage on a placeholder.
    /// Land on the first picture instead, or the first row when none has arrived yet.
    function selectFirst() {
        if (!pane || pane.listing.uri !== pane.uri) return
        if (current >= 0 || pane.listing.count === 0) return
        for (let i = 0; i < pane.listing.count; i++) {
            const r = pane.listing.row(i)
            if (!r) return
            if (r.kind === "image" || r.kind === "video") { pane.selection.set(i); ensureVisible(i); return }
        }
        if (pane.listing.done) pane.selection.set(0)
    }
    Component.onCompleted: { selectFirst(); refreshRow() }
    Connections {
        target: root.pane ? root.pane.listing : null
        function onCountChanged() { root.selectFirst(); root.refreshRow(); root.syncStrip() }
        function onReset() { root.selectFirst(); root.refreshRow(); root.syncStrip() }
        function onRowsUpdated(first, n) { root.selectFirst(); root.refreshRow() }
    }

    // The shell drives every view through these.
    function ensureVisible(i) { strip.positionViewAtIndex(i, ListView.Contain) }
    readonly property int perRow: 1
    readonly property int pageSize: 1
    function step(d) {
        const n = pane.listing.count; if (!n) return false
        if (d < 0 && current <= 0) return false
        const i = Math.max(0, Math.min(n - 1, (current < 0 ? 0 : current + d)))
        pane.selection.set(i)
        ensureVisible(i)
        zoom = 0
        return true
    }
    function fit() { zoom = 0 }
    function actual() { zoom = 1 }
    function zoomBy(f) { zoom = Math.max(0.1, Math.min(8, (zoom || fitScale) * f)) }
    /// Air between the picture and the edges of the stage: a photograph looks better mounted
    /// than bled to the edge, and the gap is where the eye rests.
    readonly property int inset: 28
    readonly property real fitScale: img.implicitWidth > 0
        ? Math.min(1, Math.min(Math.max(1, stage.width - 2 * root.inset) / img.implicitWidth,
                               Math.max(1, stage.height - 2 * root.inset) / img.implicitHeight))
        : 1

    // The stage is darker than the rest of the window, so the picture is the brightest thing on
    // screen whatever the theme.
    Rectangle {
        width: parent.width; height: root.stageHeight
        color: Qt.darker(Kiki.Theme.bgDark, 1.25)
    }

    /// The picture on screen before this one, kept while the new one loads so the change is a
    /// fade rather than a blink. Paging quickly just replaces it; nothing queues up.
    property string ghostSource: ""
    onSourceChanged: {
        if (img.status === Image.Ready && img.source != "") {
            ghost.source = img.source
            ghost.opacity = 1
        }
        img.opacity = 0
    }

    Flickable {
        id: stage
        width: parent.width; height: root.stageHeight
        contentWidth: Math.max(width, img.width); contentHeight: Math.max(height, img.height)
        clip: true; boundsBehavior: Flickable.StopAtBounds

        Image {
            id: ghost
            anchors.fill: parent
            anchors.margins: root.inset
            fillMode: Image.PreserveAspectFit
            smooth: true; mipmap: true; cache: false; asynchronous: true
            opacity: 0
            visible: opacity > 0
            Behavior on opacity { NumberAnimation { duration: 180; easing.type: Easing.InOutQuad } }
        }
        // A hairline around the picture, so a dark photograph still has an edge against the mat.
        Rectangle {
            visible: img.visible && img.status === Image.Ready
            x: img.x - 1; y: img.y - 1; width: img.width + 2; height: img.height + 2
            color: "transparent"; radius: 3
            border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.10)
        }
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
            opacity: 0
            Behavior on opacity { NumberAnimation { duration: 180; easing.type: Easing.InOutQuad } }
            onStatusChanged: if (status === Image.Ready) { img.opacity = 1; ghost.opacity = 0 }
            Component.onCompleted: if (status === Image.Ready) opacity = 1
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

    // The bar under the picture: play, pause, and the two things a slideshow needs to know.
    Item {
        id: controls
        y: stage.height; width: parent.width; height: root.controlsHeight

        // A pill under the buttons, so they read as one control over a picture of any colour.
        Rectangle {
            anchors.centerIn: parent
            width: buttons.width + 20; height: 34; radius: height / 2
            color: Qt.rgba(Kiki.Theme.surface.r, Kiki.Theme.surface.g, Kiki.Theme.surface.b, 0.55)
            border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.06)
        }
        Row {
            id: buttons
            anchors.centerIn: parent
            spacing: 6

            UI.ToggleButton {
                objectName: "gallery-play"
                icon: root.playing ? "pause" : "play"
                tip: root.playing ? "Pause" : "Play"
                onClicked: root.togglePlay()
            }
            UI.ToggleButton {
                objectName: "gallery-settings"
                icon: "gear"
                tip: "Slideshow"
                onClicked: slideOptions.visible = !slideOptions.visible
            }
        }

        Text {
            visible: root.playing
            anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter
            text: root.slideDelay + "s" + (root.slideLoop ? " · loop" : "")
            color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
        }
    }

    // Slideshow options, over the bar rather than in a window of their own.
    Rectangle {
        id: slideOptions
        objectName: "gallery-options"
        visible: false
        width: 260; height: opts.height + 24; radius: 8
        x: Math.round((root.width - width) / 2)
        y: Math.max(8, controls.y - height - 8)
        color: Kiki.Theme.bg
        border.width: 1; border.color: Kiki.Theme.line
        z: 5
        MouseArea { anchors.fill: parent }

        Column {
            id: opts
            y: 12; width: parent.width; spacing: 10

            Text {
                x: 14; text: "SLIDESHOW"; color: Kiki.Theme.muted
                font.family: Kiki.Theme.mono; font.pixelSize: 10; font.bold: true; font.letterSpacing: 1
            }
            Row {
                x: 14; spacing: 8
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Every"; color: Kiki.Theme.fgDim
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                }
                Repeater {
                    model: [2, 4, 8, 15]
                    delegate: Rectangle {
                        required property int modelData
                        objectName: "delay-" + modelData
                        width: 38; height: 24; radius: 12
                        color: root.slideDelay === modelData ? Kiki.Theme.accent : "transparent"
                        border.width: 1
                        border.color: root.slideDelay === modelData ? Kiki.Theme.accent : Kiki.Theme.gutter
                        Text {
                            anchors.centerIn: parent; text: modelData + "s"
                            color: root.slideDelay === modelData ? Kiki.Theme.bg : Kiki.Theme.fgDim
                            font.family: Kiki.Theme.mono; font.pixelSize: 11
                        }
                        MouseArea { anchors.fill: parent; onClicked: Kiki.Settings.set("view", "slideshowDelay", modelData) }
                    }
                }
            }
            Row {
                x: 14; spacing: 8
                Rectangle {
                    objectName: "slide-loop"
                    width: 16; height: 16; radius: 3; anchors.verticalCenter: parent.verticalCenter
                    color: root.slideLoop ? Kiki.Theme.accent : Kiki.Theme.bgDark
                    border.width: 1; border.color: root.slideLoop ? Kiki.Theme.accent : Kiki.Theme.gutter
                    UI.Icon { visible: root.slideLoop; anchors.centerIn: parent; name: "check"; size: 10; strokeWidth: 2.5; color: Kiki.Theme.bg }
                    MouseArea { anchors.fill: parent; onClicked: Kiki.Settings.set("view", "slideshowLoop", !root.slideLoop) }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "Loop"; color: Kiki.Theme.fgDim
                    font.family: Kiki.Theme.mono; font.pixelSize: 12
                }
            }
        }
    }

    Rectangle {
        id: stripBar
        visible: root.filmstrip
        y: stage.height + root.controlsHeight; width: parent.width; height: root.stripHeight
        color: Kiki.Theme.bgDark
        Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
        ListView {
            id: strip
            // Centred while the pictures fit, filling the bar once they do not.
            height: parent.height - 12
            anchors.verticalCenter: parent.verticalCenter
            anchors.horizontalCenter: parent.horizontalCenter
            width: Math.min(parent.width - 16, Math.max(1, contentWidth))
            orientation: ListView.Horizontal; spacing: root.shotGap
            clip: true; reuseItems: true
            // A strip scrolls sideways whatever the wheel says: a mouse has only a vertical one,
            // and a trackpad's sideways swipe should move it the way the fingers go.
            WheelHandler {
                target: null
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                onWheel: event => {
                    const px = event.pixelDelta.x !== 0 ? event.pixelDelta.x : event.angleDelta.x / 2
                    const py = event.pixelDelta.y !== 0 ? event.pixelDelta.y : event.angleDelta.y / 2
                    const d = px !== 0 ? px : py
                    if (d === 0) { event.accepted = false; return }
                    strip.contentX = Math.max(0, Math.min(strip.contentX + d, Math.max(0, strip.contentWidth - strip.width)))
                    event.accepted = true
                }
            }
            model: root.pane ? root.pane.listing.count : 0
            onContentXChanged: root.syncStrip()
            onWidthChanged: root.syncStrip()
            Component.onCompleted: root.syncStrip()
            Connections { target: root.pane ? root.pane.listing : null; function onReset() { strip.forceLayout() } }
            delegate: Rectangle {
                id: shot
                required property int index
                property var r: root.pane.listing.row(index)
                width: root.shotWidth; height: strip.height; radius: 8
                color: index === root.current ? Kiki.Theme.surface : Qt.rgba(1, 1, 1, 0.03)
                border.width: index === root.current ? 2 : 1
                border.color: index === root.current ? Kiki.Theme.accent : Qt.rgba(1, 1, 1, 0.05)
                Connections { target: root.pane.listing; function onRowsUpdated(first, n) { if (shot.index >= first && shot.index < first + n) shot.r = root.pane.listing.row(shot.index) } }
                Image {
                    visible: shot.r && shot.r.thumb
                    anchors.fill: parent; anchors.margins: 5
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
