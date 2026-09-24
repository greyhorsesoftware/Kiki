import QtQuick
import ".." as Kiki

Rectangle {
    id: b
    property string text: ""
    property bool primary: false
    property bool enabled: true
    /// The card's size (owner, 2026-09-24): shorter, tighter, a smaller face.
    property bool small: false
    signal clicked()
    height: small ? 24 : 30; width: t.implicitWidth + (small ? 24 : 32); radius: 2
    activeFocusOnTab: enabled
    // Under the pointer the box lights a little; pressed, the accent is on the edge and the face
    // darkens for the length of the press — a button that gives nothing back looks broken.
    readonly property bool pressed: press.pressed
    readonly property bool hovered: press.containsMouse
    color: primary ? (pressed ? Qt.darker(Kiki.Theme.accent, 1.25) : Kiki.Theme.accent)
                   : (pressed ? Kiki.Theme.bgDark : (activeFocus || hovered ? Kiki.Theme.surface : "transparent"))
    border.width: primary && !activeFocus && !pressed ? 0 : 1
    border.color: activeFocus || pressed ? Kiki.Theme.accent : Kiki.Theme.gutter
    Keys.onReturnPressed: if (b.enabled) b.clicked()
    Keys.onEnterPressed: if (b.enabled) b.clicked()
    Keys.onSpacePressed: if (b.enabled) b.clicked()
    opacity: enabled ? 1 : 0.5
    Text { id: t; anchors.centerIn: parent; text: b.text; color: b.primary ? Kiki.Theme.bg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: small ? 12 : Kiki.Theme.fontSize; font.bold: b.primary }
    MouseArea { id: press; anchors.fill: parent; enabled: b.enabled; hoverEnabled: true; onClicked: b.clicked() }
}
