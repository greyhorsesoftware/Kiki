import QtQuick
import ".." as Kiki

Rectangle {
    id: b
    property string text: ""
    property bool primary: false
    property bool enabled: true
    signal clicked()
    height: 30; width: t.implicitWidth + 32; radius: 2
    color: primary ? Kiki.Theme.accent : "transparent"
    border.width: primary ? 0 : 1; border.color: Kiki.Theme.gutter
    opacity: enabled ? 1 : 0.5
    Text { id: t; anchors.centerIn: parent; text: b.text; color: b.primary ? Kiki.Theme.bg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: b.primary }
    MouseArea { anchors.fill: parent; enabled: b.enabled; onClicked: b.clicked() }
}
