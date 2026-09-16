import QtQuick
import ".." as Kiki

// One line for the last destructive job, with Undo.
Rectangle {
    id: t
    property var toast: Kiki.Jobs.toast
    visible: toast !== null
    width: row.width + 20; height: 36; radius: 2; z: 50
    color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
    Row {
        id: row; anchors.verticalCenter: parent.verticalCenter; x: 14; spacing: 14
        Text { anchors.verticalCenter: parent.verticalCenter; text: t.toast ? t.toast.text : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
        Rectangle {
            visible: t.toast && t.toast.undoable
            height: 26; width: undoRow.width + 20; radius: 2; color: Kiki.Theme.surface; anchors.verticalCenter: parent.verticalCenter
            Row { id: undoRow; anchors.centerIn: parent; spacing: 8
                Text { text: "Undo"; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                Text { text: "Ctrl+Z"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter }
            }
            MouseArea { anchors.fill: parent; onClicked: { Kiki.Jobs.undo(); Kiki.Jobs.dismissToast() } }
        }
        Rectangle { width: 1; height: 16; color: Kiki.Theme.line; anchors.verticalCenter: parent.verticalCenter }
        Icon { name: "x"; size: 12; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter; MouseArea { anchors.fill: parent; onClicked: Kiki.Jobs.dismissToast() } }
    }
}
