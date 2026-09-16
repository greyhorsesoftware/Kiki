import QtQuick
import ".." as Kiki

// Favorites and Locations. Locations gets remote entries in plan 06 and devices in plan 17.
Rectangle {
    id: sidebar
    property var favorites: []
    property var volumes: []
    property var locations: []
    signal editLocation(var location)
    signal openLocation(var location)
    signal removeLocation(string name)
    property string currentUri: ""
    signal open(string uri)
    signal addLocation()
    width: Kiki.Theme.sidebarWidth
    color: Kiki.Theme.bgDark
    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }

    Column {
        anchors.fill: parent; anchors.topMargin: 12; spacing: 12
        SidebarSection {
            title: "Favorites"
            Repeater {
                model: sidebar.favorites
                delegate: SidebarItem {
                    required property var modelData
                    icon: modelData.name === "Home" ? "home" : (modelData.name === "Downloads" ? "download" : (modelData.name === "Trash" ? "trash" : "folder"))
                    iconColor: modelData.name === "Trash" ? Kiki.Theme.fgDim : Kiki.Theme.accent
                    label: modelData.name
                    active: sidebar.currentUri === modelData.uri
                    onClicked: sidebar.open(modelData.uri)
                }
            }
        }
        SidebarSection {
            title: "Locations"; plus: true
            onPlusClicked: sidebar.addLocation()
            Repeater {
                model: sidebar.volumes
                delegate: SidebarItem {
                    required property var modelData
                    icon: "hdd"; iconColor: Kiki.Theme.fgDim
                    label: modelData.name
                    active: sidebar.currentUri === modelData.uri
                    onClicked: sidebar.open(modelData.uri)
                }
            }
            Repeater {
                model: sidebar.locations
                delegate: SidebarItem {
                    required property var modelData
                    icon: "server"; iconColor: modelData.plugin === "sftp" ? Kiki.Theme.green : Kiki.Theme.cyan
                    label: modelData.name + " · " + modelData.plugin
                    active: sidebar.currentUri.startsWith(modelData.plugin + "://" + modelData.name)
                    onClicked: sidebar.openLocation(modelData)
                    onRightClicked: sidebar.editLocation(modelData)
                }
            }
        }
    }
    // Free space for the volume holding the current pane.
    Row {
        anchors.bottom: parent.bottom; anchors.bottomMargin: 12; x: 24; spacing: 10; width: parent.width - 48
        property var vol: sidebar.volumes.length ? sidebar.volumes[0] : null
        Rectangle {
            width: parent.width - 90; height: 4; radius: 2; color: Kiki.Theme.surface; anchors.verticalCenter: parent.verticalCenter
            Rectangle { height: 4; radius: 2; color: Kiki.Theme.gutter; width: parent.vol && parent.vol.total ? parent.width * (1 - parent.vol.free / parent.vol.total) : 0 }
        }
        Text { text: parent.vol ? Kiki.Format.bytes(parent.vol.free) + " free" : ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    }
}
