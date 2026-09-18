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
    color: active || hover.containsMouse ? Kiki.Theme.surface : "transparent"
    Icon { anchors.centerIn: parent; name: btn.icon; color: btn.active ? Kiki.Theme.accent : Kiki.Theme.chrome }
    Tip { visible: btn.tip !== "" && hover.containsMouse; text: btn.tip }
    MouseArea { id: hover; anchors.fill: parent; onClicked: btn.clicked(); hoverEnabled: true }
}
