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
    property bool searchOpen: false
    signal toggleSearch()
    signal search(string text, string scope)
    signal scopeMenu()
    property var locations: []
    property var repo: null
    signal viewMenu()
    signal pathMenu()
    signal settings()
    property alias breadcrumb: crumb
    property alias searchBox: searchField
    property alias viewButton: viewButton
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
        readonly property int fixedCount: 3 + (viewButton.visible ? 1 : 0) + (bar.searchOpen ? 1 : 0)
        readonly property int fixedWidth: sidebarBtn.width + (viewButton.visible ? viewButton.width : 0)
            + searchBtn.width + gearBtn.width
        readonly property int freeWidth: Math.max(0, width - fixedWidth - spacing * fixedCount)
        // Open, the field grows out of the glass and the path gives up the room; it never takes
        // the path's place entirely.
        readonly property int searchWidth: bar.searchOpen ? Math.min(260, Math.max(0, freeWidth - 90)) : 0
        readonly property int crumbWidth: Math.max(0, freeWidth - searchWidth)
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
        // Grows out of the glass beside it, squeezing the path rather than replacing it.
        SearchBox {
            id: searchField
            visible: bar.searchOpen
            width: row.searchWidth
            Behavior on width { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
            anchors.verticalCenter: parent.verticalCenter
            placeholder: "Search " + (Kiki.Format.crumbs(bar.pane.uri, bar.home).slice(-1)[0] || "")
            scopes: bar.locations.map(l => ({ id: l.name, label: l.name }))
            onChanged: text => bar.search(text, searchField.scope)
            onScopeMenu: bar.scopeMenu()
        }
        ToggleButton {
            id: searchBtn
            anchors.verticalCenter: parent.verticalCenter
            icon: "search"; tip: "Search (/)"
            onClicked: bar.toggleSearch()
        }
        ViewSwitcher { id: viewButton; visible: bar.width >= 360; anchors.verticalCenter: parent.verticalCenter; view: bar.pane.view; onMenu: bar.viewMenu() }
        ToggleButton { id: gearBtn; anchors.verticalCenter: parent.verticalCenter; icon: "gear"; tip: "Settings (Ctrl+,)"; onClicked: bar.settings() }
    }
}
