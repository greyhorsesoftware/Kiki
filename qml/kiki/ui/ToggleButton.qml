import QtQuick
import ".." as Kiki

// A 34px square toolbar button with an icon; `active` lights it.
Rectangle {
    id: btn
    property string icon: "info"
    property bool active: false
    property string tip: ""
    signal clicked()
    width: 34; height: 34; radius: 2
    color: active ? Kiki.Theme.surface : Kiki.Theme.bgDark
    border.width: 1; border.color: active ? Kiki.Theme.accent : Kiki.Theme.line
    Icon { anchors.centerIn: parent; name: btn.icon; color: btn.active ? Kiki.Theme.accent : Kiki.Theme.muted }
    MouseArea { anchors.fill: parent; onClicked: btn.clicked(); hoverEnabled: true }
}
