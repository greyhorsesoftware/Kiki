import QtQuick
import ".." as Kiki

// Favorites and Locations. Locations gets remote entries in plan 06 and devices in plan 17.
Rectangle {
    id: sidebar
    property var favorites: []
    property var volumes: []
    property var locations: []
    property var devices: []
    signal ejectDevice(var device)
    signal deviceMenu(var device)
    signal editLocation(var location)
    signal openLocation(var location)
    signal removeLocation(string name)
    property string currentUri: ""
    signal open(string uri)
    signal addLocation()
    signal dropOn(string uri, var drop)          // files dropped on a favorite or volume
    signal addFavorites(var uris)                // folders dropped on the Favorites header
    signal volumeMenu(var volume)
    signal mountVolume(var volume)
    // Keyboard focus (plan 23): a highlighted row across every section; Enter opens it.
    property int keyIndex: -1
    readonly property var entries: favorites.map(f => ({ kind: "favorite", uri: f.uri, item: f })).concat(volumes.map(v => ({ kind: "volume", uri: v.uri, item: v })), locations.map(l => ({ kind: "location", uri: l.remoteUri, item: l })), devices.map(d => ({ kind: "device", uri: d.uri, item: d })))
    function moveKey(delta) { if (!entries.length) return; keyIndex = keyIndex < 0 ? (delta > 0 ? 0 : entries.length - 1) : Math.max(0, Math.min(entries.length - 1, keyIndex + delta)) }
    function activateKey() {
        const e = entries[keyIndex]; if (!e) return
        if (e.kind === "location") sidebar.openLocation(e.item)
        else if (e.kind === "volume" && e.item.mounted === false) sidebar.mountVolume(e.item)
        else if (!(e.kind === "device" && e.item.busy)) sidebar.open(e.uri)
    }
    function keyOffset(kind, i) { let o = 0; if (kind !== "favorite") o += favorites.length; if (kind === "location" || kind === "device") o += volumes.length; if (kind === "device") o += locations.length; return o + i }
    width: Kiki.Theme.sidebarWidth
    color: Kiki.Theme.bgDark
    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }

    Column {
        id: sections
        width: parent.width; y: 12; spacing: 12
        SidebarSection {
            id: favSection
            title: "Favorites"
            // Dropping folders on the header adds them as favorites.
            DropArea {
                anchors.fill: parent; z: -1; keys: ["text/uri-list"]
                onDropped: drop => { const urls = drop.hasUrls ? drop.urls.map(u => u.toString()) : []; if (urls.length) { drop.accept(Qt.LinkAction); sidebar.addFavorites(urls) } }
                Rectangle { anchors.fill: parent; color: "transparent"; border.width: 1; border.color: Kiki.Theme.accent; visible: parent.containsDrag }
            }
            Repeater {
                model: sidebar.favorites
                delegate: SidebarItem {
                    required property var modelData
                    required property int index
                    icon: modelData.name === "Home" ? "home" : (modelData.name === "Downloads" ? "download" : (modelData.name === "Trash" ? "trash" : "folder"))
                    iconColor: modelData.name === "Trash" ? Kiki.Theme.fgDim : Kiki.Theme.accent
                    label: modelData.name
                    keyed: sidebar.keyIndex === sidebar.keyOffset("favorite", index)
                    active: sidebar.currentUri === modelData.uri
                    droppable: true
                    onDropped: drop => { if (modelData.uri.startsWith("trash://")) { const urls = drop.hasUrls ? drop.urls.map(u => u.toString()) : []; if (urls.length) { drop.accept(Qt.MoveAction); Kiki.Jobs.submit({ op: "trash", items: urls }) } } else sidebar.dropOn(modelData.uri, drop) }
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
                    required property int index
                    keyed: sidebar.keyIndex === sidebar.keyOffset("volume", index)
                    icon: modelData.removable ? "usb" : "hdd"; iconColor: modelData.mounted === false ? Kiki.Theme.gutter : Kiki.Theme.fgDim
                    label: modelData.name + (modelData.mounted === false ? "  ·  not mounted" : "")
                    active: modelData.uri && sidebar.currentUri === modelData.uri
                    droppable: modelData.mounted !== false
                    onDropped: drop => sidebar.dropOn(modelData.uri, drop)
                    onClicked: modelData.mounted === false ? sidebar.mountVolume(modelData) : sidebar.open(modelData.uri)
                    onRightClicked: sidebar.volumeMenu(modelData)
                }
            }
            Repeater {
                model: sidebar.locations
                delegate: SidebarItem {
                    required property var modelData
                    required property int index
                    keyed: sidebar.keyIndex === sidebar.keyOffset("location", index)
                    icon: "server"; iconColor: modelData.plugin === "sftp" ? Kiki.Theme.green : Kiki.Theme.cyan
                    label: modelData.name + " · " + modelData.plugin
                    active: sidebar.currentUri.startsWith(modelData.plugin + "://" + modelData.name)
                    onClicked: sidebar.openLocation(modelData)
                    onRightClicked: sidebar.editLocation(modelData)
                }
            }
        }
    }
    // Devices (plan 17): present only while a phone or camera is plugged in.
    Column {
        anchors.top: sections.bottom; anchors.topMargin: 12; width: parent.width; spacing: 12
        visible: sidebar.devices.length > 0
        SidebarSection {
            title: "Devices"
            Repeater {
                model: sidebar.devices
                delegate: SidebarItem {
                    id: devItem
                    required property var modelData
                    required property int index
                    keyed: sidebar.keyIndex === sidebar.keyOffset("device", index)
                    icon: modelData.kind === "ptp" ? "image" : "phone"; iconColor: modelData.busy ? Kiki.Theme.yellow : (modelData.connected ? Kiki.Theme.green : Kiki.Theme.fgDim)
                    label: modelData.name + (modelData.busy ? "  ·  in use by " + modelData.busy : "")
                    active: sidebar.currentUri.startsWith(modelData.uri.replace(/\/$/, ""))
                    droppable: !modelData.busy
                    onDropped: drop => sidebar.dropOn(modelData.uri, drop)
                    onClicked: if (!modelData.busy) sidebar.open(modelData.uri)
                    onRightClicked: sidebar.deviceMenu(modelData)
                    // eject on hover
                    Rectangle {
                        anchors.right: parent.right; anchors.rightMargin: 6; anchors.verticalCenter: parent.verticalCenter; width: 20; height: 20; radius: 2
                        visible: devItem.hovered || ejectHover.containsMouse; color: ejectHover.containsMouse ? Kiki.Theme.surface : "transparent"
                        Icon { anchors.centerIn: parent; name: "eject"; size: 12; color: Kiki.Theme.fgDim }
                        MouseArea { id: ejectHover; anchors.fill: parent; hoverEnabled: true; onClicked: sidebar.ejectDevice(modelData) }
                    }
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
