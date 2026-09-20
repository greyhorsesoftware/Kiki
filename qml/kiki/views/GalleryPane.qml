import QtQuick
import ".." as Kiki
import "../ui" as UI

// Gallery (plan 27): one picture filling the pane over a filmstrip of its neighbours. The
// listing, its order and the selection are the other views' — only the drawing differs.
Item {
    id: root
    // The gallery shows one folder: anything dropped on it, anywhere, goes into that folder.
    DropArea {
        objectName: "gallery-drop-background"
        anchors.fill: parent; z: -1
        keys: ["text/uri-list"]
        enabled: !!root.pane && !root.pane.isTrash
        onDropped: drop => root.pane.dropInto(root.pane.uri, drop)
    }
    property Kiki.Pane pane
    property string home: ""
    signal activate(int index)
    signal contextMenu(int index, point pos)

    property bool filmstrip: true
    property real zoom: 0                       // 0 = fit the stage
    readonly property int stripHeight: filmstrip ? 84 : 0
    /// The slideshow controls float over the middle of the picture while the pointer is on it
    /// (the same as the info panel's video buttons), so they take no room of their own: the
    /// picture has the whole height above the filmstrip.
    readonly property int controlsHeight: 0
    /// What the picture has to itself.
    readonly property int stageHeight: Math.max(0, height - stripHeight - controlsHeight)

    // ---------------------------------------------------------------- slideshow
    property bool playing: false
    readonly property int slideDelay: Math.max(1, Kiki.Settings.view.slideshowDelay || 4)
    readonly property bool slideLoop: Kiki.Settings.view.slideshowLoop !== false
    function togglePlay() { playing = !playing }
    /// The next picture after `from`, wrapping past the end when `wrap`; -1 when there is none.
    function nextPicture(from, wrap) {
        const n = pane ? pane.listing.count : 0
        for (let k = 1; k <= n; k++) {
            const i = from + k
            if (i >= n && !wrap) return -1
            const j = ((i % n) + n) % n
            const r = pane.listing.row(j)
            if (r && (r.kind === "image" || r.kind === "video")) return j
        }
        return -1
    }
    function advance() {
        if (!pane || pane.listing.count === 0) return
        const i = nextPicture(root.current < 0 ? -1 : root.current, root.slideLoop)
        if (i < 0) { playing = false; return }
        pane.selection.set(i)
        ensureVisible(i)
        root.zoom = 0
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
    Component.onCompleted: { selectFirst(); refreshRow(); showSource() }
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
    readonly property real fitScale: front ? front.fit : 1

    // The stage is darker than the rest of the window, so the picture is the brightest thing on
    // screen whatever the theme.
    Rectangle {
        width: parent.width; height: root.stageHeight + root.controlsHeight
        color: Qt.darker(Kiki.Theme.bgDark, 1.25)
    }

    // ------------------------------------------------------------------ the fade
    // Two frames, not one. The next picture decodes in the frame that is not showing and fades in
    // over the one that is, which keeps its pixels the whole time — so there is something to fade
    // from. Reloading the outgoing picture into a second Image would decode it twice and start
    // the fade from an empty frame, which is no fade at all.
    property bool bFront: false
    readonly property Image front: bFront ? frameB : frameA
    readonly property Image back: bFront ? frameA : frameB
    /// Whether the row now current is something to draw. Worked out on the spot rather than read
    /// off `isImage`: both come from `row`, and when a binding fires there is no saying which of
    /// its siblings has caught up yet.
    function wantPicture() {
        return root.source !== "" && !!root.row && (root.row.kind === "image" || root.row.kind === "video")
    }

    /// How long the pictures took to arrive. Kept always, not only under a test: the cost of
    /// showing a photograph is the one number that says whether this view is quick, and the
    /// perf flow reads it through `galleryStats`.
    property int decodes: 0
    property int decodeMs: 0
    property int worstMs: 0
    property int lastMs: 0
    property double _asked: 0
    function stats() {
        return { decodes: root.decodes, avgMs: root.decodes ? Math.round(root.decodeMs / root.decodes) : 0,
                 lastMs: root.lastMs, worstMs: root.worstMs, current: root.current,
                 count: root.pane ? root.pane.listing.count : 0,
                 ready: root.front.status === Image.Ready && root.front.opacity > 0 }
    }

    onSourceChanged: showSource()
    function showSource() {
        if (!wantPicture()) { frameA.opacity = 0; frameB.opacity = 0; return }
        if (front.source == root.source && front.opacity === 1) return
        root._asked = Date.now()
        back.source = root.source
        if (back.status === Image.Ready) arrived(back)
    }
    /// A frame finished decoding. If it is the one waiting to come in, it becomes the picture.
    function arrived(f) {
        if (f !== root.back || !wantPicture() || f.source != root.source) return
        if (root._asked > 0) {
            root.lastMs = Date.now() - root._asked
            root.decodeMs += root.lastMs
            root.decodes += 1
            if (root.lastMs > root.worstMs) root.worstMs = root.lastMs
            root._asked = 0
        }
        root.bFront = !root.bFront          // f is the front now, and sits above the old one
        f.opacity = 1
    }
    /// The fade is over: the frame underneath has nothing left to show.
    function faded() { if (root.front.opacity === 1) root.back.opacity = 0 }

    Flickable {
        id: stage
        width: parent.width; height: root.stageHeight
        contentWidth: Math.max(width, root.front.width); contentHeight: Math.max(height, root.front.height)
        clip: true; boundsBehavior: Flickable.StopAtBounds

        Image {
            id: frameA
            z: root.bFront ? 0 : 1
            visible: opacity > 0
            asynchronous: true; smooth: true; cache: false
            fillMode: Image.PreserveAspectFit
            // Decoded at the size it is shown at, not the size it was taken at.
            sourceSize: Qt.size(Math.max(64, stage.width * 2), Math.max(64, stage.height * 2))
            /// How much this picture has to shrink to sit inside the stage; never blown up.
            readonly property real fit: implicitWidth > 0
                ? Math.min(1, Math.min(Math.max(1, stage.width - 2 * root.inset) / implicitWidth,
                                       Math.max(1, stage.height - 2 * root.inset) / implicitHeight))
                : 1
            width: root.zoom === 0 ? Math.round(implicitWidth * fit) : Math.round(implicitWidth * root.zoom)
            height: root.zoom === 0 ? Math.round(implicitHeight * fit) : Math.round(implicitHeight * root.zoom)
            x: Math.max(0, (stage.contentWidth - width) / 2)
            y: Math.max(0, (stage.contentHeight - height) / 2)
            opacity: 0
            Behavior on opacity {
                NumberAnimation {
                    duration: 260; easing.type: Easing.InOutQuad
                    onRunningChanged: if (!running) root.faded()
                }
            }
            onStatusChanged: if (status === Image.Ready) root.arrived(frameA)
            // A hairline around the picture, so a dark photograph still has an edge against the mat.
            Rectangle {
                anchors.fill: parent; anchors.margins: -1
                color: "transparent"; radius: 3
                border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.10)
            }
        }
        Image {
            id: frameB
            z: root.bFront ? 1 : 0
            visible: opacity > 0
            asynchronous: true; smooth: true; cache: false
            fillMode: Image.PreserveAspectFit
            // Decoded at the size it is shown at, not the size it was taken at.
            sourceSize: Qt.size(Math.max(64, stage.width * 2), Math.max(64, stage.height * 2))
            /// How much this picture has to shrink to sit inside the stage; never blown up.
            readonly property real fit: implicitWidth > 0
                ? Math.min(1, Math.min(Math.max(1, stage.width - 2 * root.inset) / implicitWidth,
                                       Math.max(1, stage.height - 2 * root.inset) / implicitHeight))
                : 1
            width: root.zoom === 0 ? Math.round(implicitWidth * fit) : Math.round(implicitWidth * root.zoom)
            height: root.zoom === 0 ? Math.round(implicitHeight * fit) : Math.round(implicitHeight * root.zoom)
            x: Math.max(0, (stage.contentWidth - width) / 2)
            y: Math.max(0, (stage.contentHeight - height) / 2)
            opacity: 0
            Behavior on opacity {
                NumberAnimation {
                    duration: 260; easing.type: Easing.InOutQuad
                    onRunningChanged: if (!running) root.faded()
                }
            }
            onStatusChanged: if (status === Image.Ready) root.arrived(frameB)
            // A hairline around the picture, so a dark photograph still has an edge against the mat.
            Rectangle {
                anchors.fill: parent; anchors.margins: -1
                color: "transparent"; radius: 3
                border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.10)
            }
        }
        // Anything that is not a picture, or has not loaded, keeps its kind icon.
        Column {
            visible: !frameA.visible && !frameB.visible
            anchors.centerIn: parent; spacing: 12
            UI.Icon {
                anchors.horizontalCenter: parent.horizontalCenter
                name: root.row ? root.row.kind : "image"; size: 96; strokeWidth: 1
                color: Kiki.Theme.kindColor(root.row ? root.row.kind : "image")
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: !root.pane || root.pane.listing.count === 0 ? "No pictures here"
                    : (root.row ? (root.back.status === Image.Loading ? "loading…" : root.row.name) : "")
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

    // Over the picture: play, pause, and the two things a slideshow needs to know. Shown while
    // the pointer is on the picture, and while the options it opens are up.
    Item {
        id: controls
        objectName: "gallery-controls"
        width: stage.width; height: stage.height
        // A HoverHandler, not a MouseArea: it stays hovered over the buttons inside it (their
        // own areas would take the pointer from a MouseArea and the pill would blink out), and
        // it leaves the picture's clicks, drags and wheel to the stage underneath.
        HoverHandler { id: overPicture }
        readonly property bool shown: overPicture.hovered || slideOptions.visible

        // A pill under the buttons, so they read as one control over a picture of any colour.
        Rectangle {
            objectName: "gallery-pill"
            visible: controls.shown
            anchors.centerIn: buttons
            width: buttons.width + 20; height: buttons.height + 2; radius: height / 2
            color: Qt.rgba(0, 0, 0, 0.55)
            border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.25)
        }
        Row {
            id: buttons
            visible: controls.shown
            anchors.centerIn: parent
            spacing: 6

            UI.ToggleButton {
                objectName: "gallery-play"
                flat: true; iconSize: 24; restColor: "white"
                icon: root.playing ? "pause" : "play"
                tip: root.playing ? "Pause" : "Play"
                onClicked: root.togglePlay()
            }
            UI.ToggleButton {
                objectName: "gallery-settings"
                flat: true; iconSize: 24; restColor: "white"
                icon: "gear"
                tip: "Slideshow"
                onClicked: slideOptions.visible = !slideOptions.visible
            }
            // How the slideshow is running, in the pill with the button that runs it.
            Text {
                visible: root.playing
                anchors.verticalCenter: parent.verticalCenter
                rightPadding: 6
                text: root.slideDelay + "s" + (root.slideLoop ? " · loop" : "")
                color: "white"; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
        }
    }

    // Slideshow options, over the bar rather than in a window of their own.
    Rectangle {
        id: slideOptions
        objectName: "gallery-options"
        visible: false
        width: 260; height: opts.height + 24; radius: 8
        x: Math.round((root.width - width) / 2)
        // Above the pill, which sits in the middle of the picture.
        y: Math.max(8, Math.round(stage.height / 2) - 24 - height - 8)
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
