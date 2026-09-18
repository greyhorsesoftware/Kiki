import QtQuick
import ".." as Kiki

Rectangle {
    id: b
    property string text: ""
    property bool primary: false
    property bool enabled: true
    signal clicked()
    height: 30; width: t.implicitWidth + 32; radius: 2
    activeFocusOnTab: enabled
    color: primary ? Kiki.Theme.accent : (activeFocus ? Kiki.Theme.surface : "transparent")
    border.width: primary && !activeFocus ? 0 : 1
    border.color: activeFocus ? Kiki.Theme.accent : Kiki.Theme.gutter
    Keys.onReturnPressed: if (b.enabled) b.clicked()
    Keys.onEnterPressed: if (b.enabled) b.clicked()
    Keys.onSpacePressed: if (b.enabled) b.clicked()
    opacity: enabled ? 1 : 0.5
    Text { id: t; anchors.centerIn: parent; text: b.text; color: b.primary ? Kiki.Theme.bg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: b.primary }
    MouseArea { anchors.fill: parent; enabled: b.enabled; onClicked: b.clicked() }
}
