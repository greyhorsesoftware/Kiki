import QtQuick
import ".." as Kiki

// A 34px square toolbar button with an icon; `active` lights it.
Rectangle {
    id: btn
    property string icon: "info"
    property bool active: false
    property string tip: ""
    property int iconSize: 16
    /// Over a picture: no box behind the icon — the icon itself lights under the pointer.
    property bool flat: false
    /// The icon's colour at rest when `flat` (white reads on a picture; chrome does not).
    property color restColor: Kiki.Theme.chrome
    readonly property bool hovered: hover.containsMouse
    signal clicked()
    width: Math.max(34, iconSize + 18); height: width; radius: 2
    // Disabled, it dims and takes no clicks (the info button in columns view, which has an
    // info column of its own).
    opacity: enabled ? 1 : 0.35
    color: !flat && (active || hover.containsMouse) ? Kiki.Theme.surface : "transparent"
    Icon {
        objectName: "icon"
        anchors.centerIn: parent; name: btn.icon; size: btn.iconSize
        color: btn.active || (btn.flat && hover.containsMouse) ? Kiki.Theme.accent : (btn.flat ? btn.restColor : Kiki.Theme.chrome)
    }
    Tip { visible: btn.tip !== "" && hover.containsMouse; text: btn.tip }
    MouseArea { id: hover; anchors.fill: parent; enabled: btn.enabled; onClicked: btn.clicked(); hoverEnabled: btn.enabled }
}
