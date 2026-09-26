import QtQuick
import ".." as Kiki

// A picture in the Quick Look window (docs/0.2.0/05-quicklook.md, 3): the file itself through
// `Image`, decoded off the render thread with its EXIF rotation applied; the row's thumbnail —
// the daemon's, when it has made one — stands in until the picture is there, as the gallery
// does. The gallery's keys, so there is nothing new to learn: the wheel and + - zoom, 0 fits,
// 1 is actual size; a picture larger than the window is moved about by dragging it.
Item {
    id: root
    property string uri: ""
    property string name: ""
    property var row: null
    /// The picture's own size once it is decoded, for the window to size itself by. Zero until then.
    property size natural: Qt.size(0, 0)
    /// Air between the picture and the window's edge: a photograph looks better mounted than
    /// bled to the edge. The window adds it when it sizes itself to the picture.
    readonly property int inset: 16
    property real zoom: 0                       // 0 = fit the window
    /// How much the picture has to shrink to sit inside the window; never blown up.
    readonly property real fitScale: pic.implicitWidth > 0
        ? Math.min(1, Math.min(Math.max(1, width - 2 * inset) / pic.implicitWidth, Math.max(1, height - 2 * inset) / pic.implicitHeight))
        : 1
    readonly property real shownScale: zoom === 0 ? fitScale : zoom
    focus: true

    function fit() { zoom = 0 }
    function actual() { zoom = 1 }
    function zoomBy(f) { zoom = Math.max(0.1, Math.min(8, (zoom || fitScale) * f)) }
    // Another picture starts fitted: a zoom chosen for one is not a choice about the next.
    onUriChanged: zoom = 0

    Keys.onPressed: event => {
        switch (event.key) {
        case Qt.Key_0: fit(); break
        case Qt.Key_1: actual(); break
        case Qt.Key_Plus: case Qt.Key_Equal: zoomBy(1.25); break
        case Qt.Key_Minus: zoomBy(0.8); break
        default: return
        }
        event.accepted = true
    }

    // Darker than the rest of the window, so the picture is the brightest thing in it.
    Rectangle { anchors.fill: parent; color: Qt.darker(Kiki.Theme.bgDark, 1.25) }

    Flickable {
        id: stage
        objectName: "quicklook-stage"
        anchors.fill: parent
        contentWidth: Math.max(width, pic.width + 2 * root.inset)
        contentHeight: Math.max(height, pic.height + 2 * root.inset)
        clip: true; boundsBehavior: Flickable.StopAtBounds
        // Only a picture bigger than the window has anywhere to be dragged to.
        interactive: contentWidth > width || contentHeight > height

        // The thumbnail, blown up soft, where the picture will be, while the picture is on its way.
        Image {
            id: preview
            objectName: "quicklook-preview"
            visible: pic.status !== Image.Ready && source != ""
            source: root.row && root.row.thumb ? "file://" + root.row.thumb : ""
            x: stage.contentX + root.inset; y: stage.contentY + root.inset
            width: stage.width - 2 * root.inset; height: stage.height - 2 * root.inset
            fillMode: Image.PreserveAspectFit
            sourceSize: Qt.size(256, 256); asynchronous: true; smooth: true; cache: true
        }
        Image {
            id: pic
            objectName: "quicklook-picture"
            source: root.uri
            asynchronous: true; autoTransform: true; smooth: true; cache: false
            // Mipmaps only while it is drawn smaller than it is: they cost a re-upload, and a
            // picture at or above actual size has nothing to gain from them.
            mipmap: root.shownScale < 1
            // No fill mode, so this is a ceiling and not a size to scale to (see the gallery's
            // note): a picture is decoded whole up to 4096 a side — a 50-megapixel photograph
            // decoded whole is a quarter of a gigabyte, and past this it is shown fitted inside.
            sourceSize: Qt.size(4096, 4096)
            width: Math.round(implicitWidth * root.shownScale)
            height: Math.round(implicitHeight * root.shownScale)
            x: Math.max(root.inset, Math.round((stage.contentWidth - width) / 2))
            y: Math.max(root.inset, Math.round((stage.contentHeight - height) / 2))
            visible: status === Image.Ready
            onStatusChanged: root.natural = status === Image.Ready ? Qt.size(implicitWidth, implicitHeight) : Qt.size(0, 0)
            // A hairline around the picture, so a dark photograph still has an edge against the mat.
            Rectangle {
                anchors.fill: parent; anchors.margins: -1
                color: "transparent"; radius: 3
                border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.10)
            }
        }
        // The wheel zooms, as in the gallery; it never scrolls the stage, which is dragged.
        WheelHandler {
            acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
            onWheel: event => {
                const d = event.angleDelta.y !== 0 ? event.angleDelta.y : event.pixelDelta.y
                if (d !== 0) root.zoomBy(d > 0 ? 1.1 : 1 / 1.1)
                event.accepted = true
            }
        }
    }
    // Nothing to show yet and no thumbnail to stand in; or a file that will not decode.
    Text {
        anchors.centerIn: parent
        visible: (pic.status === Image.Loading && !preview.visible) || pic.status === Image.Error
        text: pic.status === Image.Error ? Kiki.T.tr("quicklook.imageFailed") : Kiki.T.tr("quicklook.loading")
        color: pic.status === Image.Error ? Kiki.Theme.danger : Kiki.Theme.muted
        font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
    }
}
