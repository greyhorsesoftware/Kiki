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
    signal dropOnTrash(var uris)
    signal addFavorites(var uris, int index)     // dropped into the Favorites list at `index`
    signal favoriteMenu(int index, point pos)
    signal volumeMenu(var volume)
    signal mountVolume(var volume)
    // Keyboard focus (plan 23): a highlighted row across every section; Enter opens it.
    property int keyIndex: -1
    /// Rail style (Settings → General): a 44px column of icons that widens on hover.
    property bool compact: false
    readonly property bool hovered: railHover.hovered
    readonly property var entries: favorites.map(f => ({ kind: "favorite", uri: f.uri, item: f })).concat([{ kind: "trash", uri: "trash:///", item: { name: "Trash", uri: "trash:///" } }], locations.map(l => ({ kind: "location", uri: l.remoteUri, item: l })), devices.map(d => ({ kind: "device", uri: d.uri, item: d })))
    function moveKey(delta) { if (!entries.length) return; keyIndex = keyIndex < 0 ? (delta > 0 ? 0 : entries.length - 1) : Math.max(0, Math.min(entries.length - 1, keyIndex + delta)) }
    function activateKey() {
        const e = entries[keyIndex]; if (!e) return
        if (e.kind === "location") sidebar.openLocation(e.item)
        else if (!(e.kind === "device" && e.item.busy)) sidebar.open(e.uri)
    }
    function keyOffset(kind, i) { let o = 0; if (kind !== "favorite") o += favorites.length; if (kind !== "favorite" && kind !== "trash") o += 1; if (kind === "device") o += locations.length; return o + i }
    /// A glyph for the well-known folders; anything else keeps the plain folder icon and
    /// leans on its tooltip.
    function favIcon(name) {
        switch ((name || "").toLowerCase()) {
        case "home": return "home"
        case "documents": case "docs": return "doc"
        case "downloads": return "download"
        case "pictures": case "photos": case "screenshots": case "wallpapers": return "image"
        case "videos": case "movies": return "video"
        case "music": return "music"
        case "projects": case "code": case "src": case "work": return "code"
        case "desktop": return "grid"
        case "trash": return "trash"
        }
        return "folder"
    }
    HoverHandler { id: railHover }
    width: Kiki.Theme.sidebarWidth
    color: Kiki.Theme.bgDark
    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }

    Column {
        id: sections
        width: parent.width; y: 12; spacing: 12
        // Folders dropped anywhere in Favorites are added to the list. The drop area sits under
        // the rows, so dropping onto a favourite still copies into that folder.
        Item {
            width: parent.width; height: favSection.height
            // A drop anywhere in Favorites adds to the list at the line shown between the rows.
            // It never copies into the folder under the pointer: that is what the pane is for.
            DropArea {
                id: favDrop
                anchors.fill: parent; keys: ["text/uri-list"]
                readonly property int pitch: 31          // SidebarItem's 30px plus the Column's 1px spacing
                property int insertAt: 0
                function indexAt(y) { return Math.max(0, Math.min(sidebar.favorites.length, Math.round((y - favSection.headerHeight) / pitch))) }
                onEntered: drag => insertAt = indexAt(drag.y)
                onPositionChanged: drag => insertAt = indexAt(drag.y)
                onDropped: drop => { const urls = drop.hasUrls ? drop.urls.map(u => u.toString()) : []; if (urls.length) { drop.accept(Qt.LinkAction); sidebar.addFavorites(urls, insertAt) } }
            }
        SidebarSection {
            id: favSection
            compact: sidebar.compact
            title: "Favorites"
            Repeater {
                model: sidebar.favorites
                delegate: SidebarItem {
                    required property var modelData
                    required property int index
                    compact: sidebar.compact
                    icon: sidebar.favIcon(modelData.name)
                    // Favourites are furniture: the one you are in is the accent, the rest match.
                    label: modelData.name
                    keyed: sidebar.keyIndex === sidebar.keyOffset("favorite", index)
                    active: sidebar.currentUri === modelData.uri
                    droppable: false
                    onClicked: sidebar.open(modelData.uri)
                    onRightClicked: sidebar.favoriteMenu(index, mapToItem(null, width / 2, height))
                }
            }
        }
            // The insertion mark: a dot on the left end of a line, centred on the gap the item
            // would drop into.
            Item {
                visible: favDrop.containsDrag
                x: 10; width: parent.width - 20; height: 8
                y: favSection.headerHeight + favDrop.insertAt * favDrop.pitch - 4
                Rectangle {
                    id: insertDot
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    width: 8; height: 8; radius: 4
                    color: "transparent"; border.width: 2; border.color: Kiki.Theme.accent
                }
                Rectangle {
                    anchors.left: insertDot.right; anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    height: 2; color: Kiki.Theme.accent
                }
            }
        }
        SidebarSection {
            compact: sidebar.compact
            title: "Locations"; plus: true
            onPlusClicked: sidebar.addLocation()
            SidebarItem {
                compact: sidebar.compact
                icon: "trash"; label: "Trash"
                keyed: sidebar.keyIndex === sidebar.keyOffset("trash", 0)
                active: sidebar.currentUri.startsWith("trash://")
                droppable: true
                onDropped: drop => { const urls = drop.hasUrls ? drop.urls.map(u => u.toString()) : []; if (urls.length) { drop.accept(Qt.MoveAction); sidebar.dropOnTrash(urls) } }
                onClicked: sidebar.open("trash:///")
            }
            Repeater {
                model: sidebar.locations
                delegate: SidebarItem {
                    required property var modelData
                    required property int index
                    compact: sidebar.compact
                    keyed: sidebar.keyIndex === sidebar.keyOffset("location", index)
                    icon: "server"
                    image: modelData.image || ""
                    label: modelData.name + " · " + modelData.plugin
                    active: sidebar.currentUri.startsWith(modelData.plugin + "://" + modelData.name)
                    onClicked: sidebar.openLocation(modelData)
                    onRightClicked: sidebar.editLocation(modelData)
                }
            }
            // The section header carries the +, and the rail has no headers.
            SidebarItem {
                visible: sidebar.compact
                compact: true
                icon: "plus"; label: "Add location"; tipText: "Add location · Ctrl+Shift+L"
                onClicked: sidebar.addLocation()
            }
        }
    }
    // Devices (plan 17): present only while a phone or camera is plugged in.
    Column {
        anchors.top: sections.bottom; anchors.topMargin: 12; width: parent.width; spacing: 12
        visible: sidebar.devices.length > 0
        SidebarSection {
            compact: sidebar.compact
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
    // Free space for the volume holding the current pane; no room for it on the rail.
    Row {
        visible: !sidebar.compact
        anchors.bottom: parent.bottom; anchors.bottomMargin: 12; x: 24; spacing: 10; width: parent.width - 48
        property var vol: sidebar.volumes.length ? sidebar.volumes[0] : null
        Rectangle {
            width: parent.width - 90; height: 4; radius: 2; color: Kiki.Theme.surface; anchors.verticalCenter: parent.verticalCenter
            Rectangle { height: 4; radius: 2; color: Kiki.Theme.gutter; width: parent.vol && parent.vol.total ? parent.width * (1 - parent.vol.free / parent.vol.total) : 0 }
        }
        Text { text: parent.vol ? Kiki.Format.bytes(parent.vol.free) + " free" : ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
    }
}
