import QtQuick
import ".." as Kiki

// Running and recent jobs with progress and cancel.
Rectangle {
    id: pop
    visible: false
    width: 380; height: Math.min(420, 12 + Math.max(1, Kiki.Jobs.list.length) * 44); radius: 2; z: 60
    color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
    function toggle() { visible = !visible }
    Column {
        anchors.fill: parent; anchors.margins: 6
        Text { visible: Kiki.Jobs.list.length === 0; x: 8; height: 32; verticalAlignment: Text.AlignVCenter; text: "No jobs yet"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
        Repeater {
            model: Kiki.Jobs.list.slice().reverse()
            delegate: Item {
                required property var modelData
                width: parent.width; height: 44
                Column {
                    x: 8; y: 6; width: parent.width - 60; spacing: 4
                    Text { width: parent.width; elide: Text.ElideRight; text: modelData.title; color: modelData.state === "failed" ? Kiki.Theme.red : Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                    Row {
                        spacing: 10; width: parent.width
                        Rectangle {
                            visible: modelData.state === "running"; width: 160; height: 4; radius: 2; color: Kiki.Theme.surface; anchors.verticalCenter: parent.verticalCenter
                            Rectangle { height: 4; radius: 2; color: Kiki.Theme.accent; width: modelData.bytesTotal ? parent.width * modelData.bytes / modelData.bytesTotal : (modelData.total ? parent.width * modelData.done / modelData.total : 0) }
                        }
                        Text { text: modelData.state === "running" ? (modelData.done + "/" + modelData.total + (modelData.bytesTotal ? " · " + Kiki.Format.bytes(modelData.bytes) + " of " + Kiki.Format.bytes(modelData.bytesTotal) : "")) : (modelData.error || modelData.state); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                    }
                }
                Rectangle {
                    visible: modelData.state === "running" || modelData.state === "queued"
                    anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter
                    width: 24; height: 24; radius: 2; color: "transparent"
                    Icon { anchors.centerIn: parent; name: "x"; size: 12; color: Kiki.Theme.muted }
                    MouseArea { anchors.fill: parent; onClicked: Kiki.Jobs.cancel(modelData.id) }
                }
            }
        }
    }
}
