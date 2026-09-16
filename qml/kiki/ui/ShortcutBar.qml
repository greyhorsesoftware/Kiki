import QtQuick
import ".." as Kiki

// The bottom strip: context keys on the left, status on the right.
Rectangle {
    id: bar
    property var keys: []        // [{key, label}]
    property string status: ""
    height: Kiki.Theme.barHeight
    color: Kiki.Theme.bg
    Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Kiki.Theme.line }
    Row {
        anchors.verticalCenter: parent.verticalCenter; x: 14; spacing: 14
        Repeater { model: bar.keys; delegate: KeyChip { required property var modelData; key: modelData.key; label: modelData.label } }
    }
    Text { anchors.right: parent.right; anchors.rightMargin: 14; anchors.verticalCenter: parent.verticalCenter; text: bar.status; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
}
