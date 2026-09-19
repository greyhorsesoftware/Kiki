import QtQuick
import ".." as Kiki

// Side by side: each pane's place icon and its own path, over its own listing; the focused pane has
// the accent underline. The title bar's path steps aside while these are up.
Rectangle {
    id: h
    property Kiki.Pane pane
    property string home: ""
    property var location: null      // the sidebar location when the pane is remote
    signal clicked()
    property bool showPath: true
    /// Right click on the path: the window builds the menu of folders above, under `crumb`.
    signal pathMenu(var crumb)
    property alias breadcrumb: crumb
    height: 34
    color: Kiki.Theme.bgDark
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: h.pane.focused ? Kiki.Theme.accent : Kiki.Theme.line }
    Row {
        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 8
        // Which kind of place this pane is in, as an icon and nothing else: one drive for this
        // machine, the green pair for a remote. No label — the path beside it already starts
        // with the remote's name, and "local" said nothing the drive does not.
        Item {
            id: badgeBox
            objectName: "pane-badge"
            readonly property bool local: h.pane ? h.pane.uri.startsWith("file://") : true
            anchors.verticalCenter: parent.verticalCenter; width: 18; height: 20
            Icon { objectName: "pane-badge-icon"; anchors.centerIn: parent; name: badgeBox.local ? "hdd" : "server"; size: 15; color: badgeBox.local ? Kiki.Theme.fgDim : Kiki.Theme.green }
        }
        // The same control the title bar has: crumbs that navigate, a path you can type, the
        // folders above on a right click.
        Breadcrumb {
            id: crumb
            objectName: "pane-crumb"
            visible: h.showPath
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, parent.width - badgeBox.width - parent.spacing)
            height: 26
            uri: h.pane ? h.pane.uri : ""; home: h.home
            onNavigate: uri => { h.clicked(); h.pane.open(uri) }
            onPathMenu: h.pathMenu(crumb)
        }
    }
    MouseArea { anchors.fill: parent; onClicked: h.clicked(); z: -1 }
}
