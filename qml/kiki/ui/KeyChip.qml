import QtQuick
import ".." as Kiki

Row {
    property string key: ""
    property string label: ""
    spacing: 5
    Rectangle {
        height: 16; width: keyText.implicitWidth + 10; radius: 2
        color: "transparent"; border.color: Kiki.Theme.gutter; border.width: 1
        Text { id: keyText; anchors.centerIn: parent; text: key; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 10 }
    }
    Text { anchors.verticalCenter: parent.verticalCenter; text: label; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
}
