import QtQuick
import ".." as Kiki

// The bottom strip: context keys on the left, status on the right.
Rectangle {
    id: bar
    property var keys: []        // [{key, label}]
    property string status: ""
    /// Room kept clear at the right for what the window puts there: the activity orb.
    property int statusInset: 14
    height: Kiki.Theme.barHeight
    color: Kiki.Theme.bg
    Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Kiki.Theme.line }
    // The chips take what the status text leaves and are cut off rather than drawn over it.
    Item {
        anchors.left: parent.left; anchors.right: status.left; anchors.rightMargin: 12
        height: parent.height; clip: true
        Row {
            anchors.verticalCenter: parent.verticalCenter; x: 14; spacing: 14
            Repeater { model: bar.keys; delegate: KeyChip { required property var modelData; key: modelData.key; label: modelData.label } }
        }
    }
    Text { id: status; anchors.right: parent.right; anchors.rightMargin: bar.statusInset; anchors.verticalCenter: parent.verticalCenter; text: bar.status; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
}
