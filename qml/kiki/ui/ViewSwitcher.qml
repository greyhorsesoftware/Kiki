import QtQuick
import ".." as Kiki

// One toolbar button showing the current view's icon; clicking opens the view menu
// (icon / list / columns, then Show hidden files).
Rectangle {
    id: sw
    property string view: "list"
    signal menu()
    width: 52; height: 34; radius: 2
    color: hover.containsMouse ? Kiki.Theme.surface : Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line
    Row {
        anchors.centerIn: parent; spacing: 4
        Icon { name: sw.view === "icon" ? "grid" : (sw.view === "columns" ? "columns" : "list"); color: Kiki.Theme.accent; anchors.verticalCenter: parent.verticalCenter }
        Icon { name: "chev-d"; size: 10; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
    }
    MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; onClicked: sw.menu() }
}
