import QtQuick
import QtQuick.Effects
import ".." as Kiki
import "../ui" as UI

// Gallery (plan 27): one picture filling the pane over a filmstrip of its neighbours. The
// listing, its order and the selection are the other views' — only the drawing differs.
Item {
    id: root
    // The gallery shows one folder: anything dropped on it, anywhere, goes into that folder.
    DropTarget {
        objectName: "gallery-drop-background"
        anchors.fill: parent; z: -1
        enabled: !!root.pane && !root.pane.isTrash
        pane: root.pane
        dest: root.pane ? root.pane.uri : ""
    }
    property Kiki.Pane pane
    property string home: ""
    signal activate(int index)
    signal contextMenu(int index, point pos)

    property bool filmstrip: true
    property real zoom: 0                       // 0 = fit the stage
    // 84 was the bar with 7 px above a tile; the current tile grows, and the owner wanted more
    // room over it (2026-09-22): 13 px each side, the tile as tall as before.
    readonly property int stripHeight: filmstrip ? 96 : 0
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
    /// A non-picture on the stage: about two fifths of the stage's shorter side, within reason.
    readonly property int stageIconSize: Math.max(128, Math.min(320, Math.round(Math.min(width, height) * 0.4)))
    readonly property int shotGap: 8
    readonly property int shotPitch: shotWidth + shotGap
    readonly property int current: pane ? pane.selection.current : -1
    property var row: null
    function refreshRow() { row = (pane && current >= 0) ? pane.listing.row(current) : null }
    // The strip follows the picture on the stage however it was chosen — a click on a tile at
    // the edge, a script, the keys — not only after the keys, which were the one caller before.
    onCurrentChanged: { refreshRow(); if (current >= 0) ensureVisible(current) }
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
    /// Where the stage was when what is on it was sent to the trash. The row goes, the selection
    /// goes with it, and without this the stage jumped back to the first picture in the folder:
    /// deleting your way through a shoot started again from the top after every one.
    property int _place: -1
    function keepPlace() { _place = current }
    function selectFirst() {
        if (!pane || pane.listing.uri !== pane.uri) return
        if (current >= 0) { return }
        if (pane.listing.count === 0) { _place = -1; return }
        if (_place >= 0) {
            // The row that followed has moved up into the gap; at the end, the one before.
            const i = Math.min(_place, pane.listing.count - 1)
            _place = -1
            pane.selection.set(i); ensureVisible(i); zoom = 0
            return
        }
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
    /// The strip keeps the current tile in view — gliding there, not jumping. `positionViewAtIndex`
    /// writes `contentX` from C++ where no Behavior sees it, so the target is worked out here and
    /// animated; the tile is centred when there is room to, so the neighbours read both ways.
    function ensureVisible(i) {
        const want = Math.max(-strip.leftMargin, Math.min(i * shotPitch - (strip.width - shotWidth) / 2, Math.max(-strip.leftMargin, strip.contentWidth - strip.width + strip.rightMargin)))
        if (Math.abs(want - strip.contentX) < 1) return
        glide.stop(); glide.from = strip.contentX; glide.to = want; glide.start()
    }
    property NumberAnimation glide: NumberAnimation { target: strip; property: "contentX"; duration: 200; easing.type: Easing.OutCubic }
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
    /// A picture zoomed past the stage is moved around by dragging it, so only a stage with
    /// nothing to pan starts a drag out of the gallery.
    readonly property bool canPan: stage.contentWidth > stage.width || stage.contentHeight > stage.height

    /// A drag leaving the gallery carries what List and Icon carry: the selection when the row
    /// pressed is in it, that row alone when it is not. `proxy` is the item whose `Drag` hands
    /// it to the compositor.
    function dragFrom(active, index, proxy) {
        if (!active) { proxy.Drag.active = false; return }
        if (!pane.selection.has(index)) pane.selection.set(index)
        proxy.Drag.mimeData = pane.dragMime(index)
        proxy.Drag.active = true
    }

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

    /// `source` as a url, which is what an Image gives back: a name with a space in it is
    /// `a%20b.jpg` in the string and `a b.jpg` read back from the frame, and comparing the two
    /// as strings said "not the one I asked for" of every picture with a space in its name —
    /// decoded, never faded in, the placeholder card left on the stage.
    property url wanted: ""
    onSourceChanged: showSource()
    function showSource() {
        slowTimer.stop(); preview.opacity = 0
        if (!wantPicture()) { frameA.opacity = 0; frameB.opacity = 0; return }
        root.wanted = root.source
        if (front.source == root.wanted && front.opacity === 1) return
        root._asked = Date.now()
        back.source = root.wanted
        if (back.status === Image.Ready) { arrived(back); return }
        // Nothing on the stage yet: the thumbnail stands in at once. Something on it: the old
        // picture stays, and the thumbnail comes over it only if the new one is slow (a big
        // file, a server) — the quick crossfade is kept for the common case.
        if (front.opacity === 0 || front.status !== Image.Ready) preview.opacity = 1
        else slowTimer.restart()
    }
    /// The picture that was asked for is taking its time: its thumbnail, blown up soft, is
    /// better than the last picture for a second, and far better than nothing.
    property Timer slowTimer: Timer { interval: 180; onTriggered: if (root.back.source == root.wanted && root.back.status !== Image.Ready) preview.opacity = 1 }
    /// A frame finished decoding. If it is the one waiting to come in, it becomes the picture.
    function arrived(f) {
        if (f !== root.back || !wantPicture() || f.source != root.wanted) return
        if (root._asked > 0) {
            root.lastMs = Date.now() - root._asked
            root.decodeMs += root.lastMs
            root.decodes += 1
            if (root.lastMs > root.worstMs) root.worstMs = root.lastMs
            root._asked = 0
        }
        root.bFront = !root.bFront          // f is the front now, and sits above the old one
        f.opacity = 1
        slowTimer.stop(); preview.opacity = 0
    }
    /// The fade is over: the frame underneath has nothing left to show.
    function faded() { if (root.front.opacity === 1) root.back.opacity = 0 }

    Flickable {
        id: stage
        objectName: "gallery-stage"
        width: parent.width; height: root.stageHeight
        contentWidth: Math.max(width, root.front.width); contentHeight: Math.max(height, root.front.height)
        clip: true; boundsBehavior: Flickable.StopAtBounds

        // ---- under the picture: the picture itself, blurred and dimmed, filling the stage.
        // A portrait no longer floats in a dark box; the mat takes the picture's own colours.
        // Made from the thumbnail — 128 px stretched over the stage is already most of a blur,
        // and costs nothing to decode — so it is there before the picture is. Pinned to the
        // viewport, not the content: it does not pan with a zoomed picture.
        Item {
            id: backdrop
            objectName: "gallery-backdrop"
            x: stage.contentX; y: stage.contentY; width: stage.width; height: stage.height
            z: -2
            visible: root.wantPicture() && thumbSource.status === Image.Ready
            Image {
                id: thumbSource
                anchors.fill: parent; visible: false
                source: root.row && root.row.thumb ? "file://" + root.row.thumb : ""
                sourceSize: Qt.size(160, 160); fillMode: Image.PreserveAspectCrop
                asynchronous: true; smooth: true; cache: true; mipmap: true
            }
            MultiEffect {
                anchors.fill: parent
                source: thumbSource
                blurEnabled: true; blur: 1.0; blurMax: 64; blurMultiplier: 2.5
                saturation: -0.25; brightness: -0.35
                opacity: 0.85
            }
        }
        // ---- the thumbnail, at the size the picture will be, while the picture is on its way.
        Image {
            id: preview
            objectName: "gallery-preview"
            z: -1
            source: thumbSource.source
            sourceSize: Qt.size(160, 160); smooth: true; cache: true; asynchronous: true
            readonly property real fit: implicitWidth > 0
                ? Math.min(Math.max(1, stage.width - 2 * root.inset) / implicitWidth, Math.max(1, stage.height - 2 * root.inset) / implicitHeight)
                : 1
            width: Math.round(implicitWidth * fit); height: Math.round(implicitHeight * fit)
            x: stage.contentX + Math.max(0, (stage.width - width) / 2); y: stage.contentY + Math.max(0, (stage.height - height) / 2)
            opacity: 0; visible: opacity > 0 && source != ""
            Behavior on opacity { NumberAnimation { duration: 180 } }
        }

        Image {
            id: frameA
            z: root.bFront ? 0 : 1
            visible: opacity > 0
            asynchronous: true; smooth: true; cache: false
            // No fill mode: the frame gives itself the picture's own proportions below, so there
            // is nothing left for one to decide — and `PreserveAspectFit` here is what made
            // `sourceSize` a size to scale TO rather than a ceiling. Measured under Qt 6.11.2: a
            // 120 × 80 icon came back 1600 px wide and a 3000 × 600 panorama 5160 px — bigger
            // than the file, and more memory than asking for nothing at all. Plain, the same
            // property is the cap it was meant to be: fitted inside, never enlarged.
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
            // The same as frameA: no fill mode, so `sourceSize` caps the decode instead of
            // setting it. See the note there.
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
            // The artwork Icon view draws for it — a folder, a document — scaled to the stage, so
            // a folder with no pictures in it is a folder to look through, not an empty state.
            UI.KindIcon {
                objectName: "gallery-stage-icon"
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !!root.row
                kind: root.row ? root.row.kind : ""
                size: root.stageIconSize
                color: Kiki.Theme.kindColor(root.row ? root.row.kind : "file")
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                objectName: "gallery-stage-label"
                text: !root.pane || root.pane.listing.count === 0 ? (root.pane && root.pane.listing.done ? "Empty folder" : "")
                    : (root.row ? (root.back.status === Image.Loading ? "loading…" : root.row.name) : "")
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
        }
        Item {
            id: stageDrag
            objectName: "gallery-stage-drag"
            Drag.dragType: Drag.Automatic
            Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
            Drag.proposedAction: Qt.MoveAction
        }
        MouseArea {
            objectName: "gallery-stage-mouse"
            anchors.fill: parent
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            drag.target: root.current >= 0 && !root.canPan ? stageDrag : null
            drag.threshold: 8
            drag.onActiveChanged: root.dragFrom(drag.active, root.current, stageDrag)
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
                tip: Kiki.T.tr("gallery.slideshow")
                onClicked: slideOptions.visible = !slideOptions.visible
            }
            // How the slideshow is running, in the pill with the button that runs it.
            Text {
                visible: root.playing
                anchors.verticalCenter: parent.verticalCenter
                rightPadding: 6
                text: Kiki.T.tr("gallery.seconds", { n: root.slideDelay }) + (root.slideLoop ? Kiki.T.tr("gallery.loopSuffix") : "")
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
                x: 14; text: Kiki.T.tr("gallery.slideshowCaps"); color: Kiki.Theme.muted
                font.family: Kiki.Theme.mono; font.pixelSize: 10; font.bold: true; font.letterSpacing: 1
            }
            Row {
                x: 14; spacing: 8
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: Kiki.T.tr("gallery.every"); color: Kiki.Theme.fgDim
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
                            anchors.centerIn: parent; text: Kiki.T.tr("gallery.seconds", { n: modelData })
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
                    text: Kiki.T.tr("gallery.loop"); color: Kiki.Theme.fgDim
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
            objectName: "gallery-strip"
            // Centred while the pictures fit, filling the bar once they do not.
            // The strip is the bar's full height and the tiles sit 13 px in from either edge, so
            // the current tile has room to grow without the strip's clip taking its edges off.
            height: parent.height
            anchors.verticalCenter: parent.verticalCenter
            anchors.horizontalCenter: parent.horizontalCenter
            // 6 px of the strip's own on either side of its content, inside its clip: the first
            // and last tiles grow too, and were losing their outer edges to it.
            width: Math.min(parent.width - 16, Math.max(1, contentWidth) + leftMargin + rightMargin)
            leftMargin: 6; rightMargin: 6
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
                    strip.contentX = Math.max(-strip.leftMargin, Math.min(strip.contentX + d, Math.max(-strip.leftMargin, strip.contentWidth - strip.width + strip.rightMargin)))
                    event.accepted = true
                }
            }
            model: root.pane ? root.pane.listing.count : 0
            onContentXChanged: root.syncStrip()
            onWidthChanged: root.syncStrip()
            // Starts at its margin, not at 0, or the first tile sits 6 px into the clip.
            Component.onCompleted: { contentX = -leftMargin; root.syncStrip() }
            onCountChanged: if (contentX < -leftMargin || contentWidth <= width) contentX = -leftMargin
            Connections { target: root.pane ? root.pane.listing : null; function onReset() { strip.forceLayout() } }
            delegate: Rectangle {
                id: shot
                required property int index
                // What a tile shows is the file at its place in the listing NOW, read again every
                // time either can have changed: the tile is handed another place (the strip
                // recycles its tiles as it scrolls), rows arrive or change, files come and go
                // (a splice announces itself as rows updated from the first change on), the
                // order changes. It was a binding that the first update replaced with a plain
                // value, so a recycled tile went on showing the file of the place it had before
                // — a picture under another's name, or a text file's icon where a picture was.
                // The picture itself is never by position: `thumb` is the daemon's path for
                // that file, and comes with the row.
                property var r: null
                // Neither the pane nor the view itself while it is being taken down: a delegate
                // outlives them by a moment, and re-reading its row then was 83 TypeErrors in a
                // session's log.
                function refresh() { r = root && root.pane ? root.pane.listing.row(index) : null }
                onIndexChanged: refresh()
                Component.onCompleted: refresh()
                ListView.onReused: refresh()
                width: root.shotWidth; height: strip.height - 26; radius: 8
                // 13 px down from the bar's top. A horizontal ListView writes its delegates' `y`
                // itself (0), so a `y:` here is overwritten; a transform is left alone.
                transform: Translate { y: 13 }
                color: index === root.current ? Kiki.Theme.surface : Qt.rgba(1, 1, 1, 0.03)
                border.width: index === root.current ? 2 : 1
                border.color: index === root.current ? (root.pane && !root.pane.focused ? Kiki.Theme.gutter : Kiki.Theme.accent) : Qt.rgba(1, 1, 1, 0.05)
                // The one on the stage comes forward a little and its neighbours step back; both
                // ease, so stepping along the strip is a movement and not a blink.
                readonly property bool current: index === root.current
                scale: current ? 1.12 : 1
                z: current ? 2 : 0
                opacity: current || root.current < 0 ? 1 : 0.72
                Behavior on scale { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
                Behavior on opacity { NumberAnimation { duration: 160 } }
                Connections {
                    target: root && root.pane ? root.pane.listing : null
                    function onRowsUpdated(first, n) { if (shot.index >= first && shot.index < first + n) shot.refresh() }
                    function onReset() { shot.refresh() }
                }
                // The thumbnail fills its tile, cropped, with the tile's corners: a strip of
                // photographs rather than a row of letterboxed icons.
                Item {
                    objectName: "strip-thumb"
                    visible: !!(shot.r && shot.r.thumb)
                    anchors.fill: parent; anchors.margins: 4
                    property string source: shot.r && shot.r.thumb ? "file://" + shot.r.thumb : ""
                    readonly property int status: tileImage.status
                    Image {
                        id: tileImage
                        anchors.fill: parent; visible: false
                        source: parent.source
                        sourceSize: Qt.size(144, 144); fillMode: Image.PreserveAspectCrop
                        asynchronous: true; smooth: true
                    }
                    Item { id: tileMask; anchors.fill: parent; visible: false; layer.enabled: true; layer.smooth: true
                        Rectangle { anchors.fill: parent; radius: 5; color: "black"; antialiasing: true } }
                    MultiEffect { anchors.fill: parent; visible: tileImage.status === Image.Ready; source: tileImage; maskEnabled: true; maskSource: tileMask; maskThresholdMin: 0.5; maskSpreadAtMin: 1.0 }
                }
                UI.KindIcon {
                    visible: !(shot.r && shot.r.thumb)
                    anchors.centerIn: parent; size: 32
                    kind: shot.r ? shot.r.kind : ""; color: Kiki.Theme.kindColor(shot.r ? shot.r.kind : "file")
                }
                Item {
                    id: shotDrag
                    objectName: "strip-drag"
                    Drag.dragType: Drag.Automatic
                    Drag.supportedActions: Qt.CopyAction | Qt.MoveAction
                    Drag.proposedAction: Qt.MoveAction
                }
                // The drag starts at 8 px, under the strip's own threshold for scrolling, so a
                // tile pulled along the strip leaves with the pointer instead of scrolling it.
                MouseArea {
                    anchors.fill: parent
                    drag.target: shotDrag; drag.threshold: 8
                    drag.onActiveChanged: root.dragFrom(drag.active, shot.index, shotDrag)
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
                    strip.contentX = Math.max(-strip.leftMargin, Math.min(strip.contentX + dx, Math.max(-strip.leftMargin, strip.contentWidth - strip.width + strip.rightMargin)))
                    event.accepted = true
                }
            }
        }
    }
}
