import QtQuick
import QtQuick.Effects

// A picture cropped to a rounded rectangle: it fills the box (cropping the longer side), and
// the corners are cut by a mask, so a square picture gets round corners rather than a frame.
Item {
    id: r
    property string source: ""
    property real radius: 5
    readonly property int status: img.status
    /// There is a picture and it loaded; false for none, still loading, or unreadable.
    readonly property bool shown: source !== "" && img.status === Image.Ready
    Image {
        id: img
        anchors.fill: parent; visible: false
        source: r.source
        sourceSize: Qt.size(Math.ceil(r.width * 2), Math.ceil(r.height * 2))
        fillMode: Image.PreserveAspectCrop; asynchronous: true; smooth: true; mipmap: true
    }
    Item {
        id: mask
        anchors.fill: parent; visible: false
        layer.enabled: true; layer.smooth: true
        Rectangle { anchors.fill: parent; radius: r.radius; color: "black"; antialiasing: true }
    }
    MultiEffect {
        anchors.fill: parent; visible: r.shown
        source: img
        maskEnabled: true; maskSource: mask
        maskThresholdMin: 0.5; maskSpreadAtMin: 1.0
    }
}
