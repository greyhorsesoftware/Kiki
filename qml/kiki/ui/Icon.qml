import QtQuick
import "../icons.js" as Icons

// One icon from the kiki set, recoloured by changing `color`.
Image {
    property string name: "file"
    property color color: "#a9b1d6"
    property int size: 16
    property real strokeWidth: 1.5
    width: size; height: size
    sourceSize: Qt.size(size * 2, size * 2)
    source: Icons.svg(name, color.toString(), size, strokeWidth)
    smooth: true
}
