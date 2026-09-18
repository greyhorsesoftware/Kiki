import QtQuick
import ".." as Kiki

Rectangle {
    id: bar
    property Kiki.Pane pane
    property string home: ""
    property bool inspector: false
    property bool split: false
    property bool mirror: false
    signal toggleMirror()
    signal toggleSplit()
    // Search lives in the header: the glass expands into a field over the path.
    signal toggleSearch()
    property var locations: []
    property var repo: null
    signal viewMenu()
    signal pathMenu()
    signal settings()
    property alias breadcrumb: crumb
    property alias viewButton: viewButton
    property alias gearButton: gearBtn
    property alias searchButton: searchBtn
    // Favorites panel toggle (far left).
    property bool sidebarShown: true
    signal toggleSidebar()
    height: Kiki.Theme.toolbarHeight
    color: Kiki.Theme.bg
    clip: true
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }

    Row {
        id: row
        anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8; spacing: 4
        // The buttons keep their natural size and the path takes the rest, so the toolbar fits
        // the window instead of spilling over the sidebar. The view button is the first to go
        // when the window is too narrow for it.
        readonly property int fixedCount: 3 + (viewButton.visible ? 1 : 0)
        readonly property int fixedWidth: sidebarBtn.width + (viewButton.visible ? viewButton.width : 0)
            + searchBtn.width + gearBtn.width
        readonly property int freeWidth: Math.max(0, width - fixedWidth - spacing * fixedCount)
        // Open, the field grows out of the glass and the path gives up the room; it never takes
        // the path's place entirely.
        readonly property int crumbWidth: freeWidth
        ToggleButton {
            id: sidebarBtn
            anchors.verticalCenter: parent.verticalCenter
            icon: "sidebar"; tip: bar.sidebarShown ? "Hide favorites (Ctrl+Shift+B)" : "Show favorites (Ctrl+Shift+B)"
            onClicked: bar.toggleSidebar()
        }
        Breadcrumb {
            id: crumb
            repo: bar.repo
            anchors.verticalCenter: parent.verticalCenter
            visible: row.crumbWidth >= 80
            width: row.crumbWidth
            uri: bar.pane.uri; home: bar.home
            onNavigate: uri => bar.pane.open(uri)
            onPathMenu: bar.pathMenu()
        }
        ToggleButton {
            id: searchBtn
            anchors.verticalCenter: parent.verticalCenter
            icon: "search"; tip: "Search everywhere (Ctrl+Shift+F)"
            onClicked: bar.toggleSearch()
        }
        ViewSwitcher { id: viewButton; visible: bar.width >= 360; anchors.verticalCenter: parent.verticalCenter; view: bar.pane.view; onMenu: bar.viewMenu() }
        // A menu, not a button: the chevron says so, the way the view switcher does.
        Rectangle {
            id: gearBtn
            objectName: "gear"
            anchors.verticalCenter: parent.verticalCenter
            width: 48; height: 34; radius: 2
            color: gearHover.containsMouse ? Kiki.Theme.surface : "transparent"
            Row {
                anchors.centerIn: parent; spacing: 4
                Icon { name: "gear"; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
                Icon { name: "chev-d"; size: 10; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
            }
            MouseArea { id: gearHover; anchors.fill: parent; hoverEnabled: true; onClicked: bar.settings() }
        }
    }
}
