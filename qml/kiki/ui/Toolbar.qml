import QtQuick
import ".." as Kiki

Rectangle {
    id: bar
    property Kiki.Pane pane
    /// The pane the PATH shows and drives; the focused one unless the window says otherwise.
    property Kiki.Pane crumbPane: pane
    /// Side by side each pane carries its own path, over its own listing, so there are two and
    /// neither is "the window's". The one up here steps aside — it keeps its room, so the
    /// buttons do not slide about when the layout is toggled.
    readonly property bool pathShown: !split
    property string home: ""
    property bool inspector: false
    property bool split: false
    property bool mirror: false
    signal toggleMirror()
    /// The server a pane is on, side by side — "" when both are on this machine. The button
    /// beside Mirror lets go of it.
    property string remoteHost: ""
    /// A server is open in a pane: what side by side is for, and the only time it is offered.
    property bool remoteOpen: false
    /// What the path shows when it is not simply the pane's folder: columns view's deepest open one.
    property string crumbUri: ""
    signal disconnect()
    signal toggleSplit()
    // Search lives in the header: the glass expands into a field over the path.
    signal toggleSearch()
    property var locations: []
    property var repo: null
    signal viewMenu()
    /// A path chosen in the breadcrumb. The window decides which pane takes it.
    signal navigate(string uri)
    signal pathMenu()
    signal settings()
    property alias breadcrumb: crumb
    property alias viewButton: viewButton
    property alias sideBySideButton: sideBySideBtn
    property alias mirrorButton: mirrorBtn
    property alias disconnectButton: disconnectBtn
    property alias gearButton: gearBtn
    property alias searchButton: searchBtn
    // Favorites panel toggle (far left).
    property bool sidebarShown: true
    signal toggleSidebar()
    height: Kiki.Theme.toolbarHeight
    color: Kiki.Theme.bg
    clip: true
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }


    // Side by side the path has stepped aside, and its room is where the one thing the layout is
    // for goes: the line with an arrow pointing each way starts a mirror run. It stands over the
    // line between the two panes — `mirrorCenterX`, which the window knows and the bar does not:
    // the sidebar pushes the panes right and the line can be dragged, so the middle of the BAR is
    // not where it is. (Tried standing it over the middle of the panes instead, so it would stay
    // put during a drag: with a remembered ratio of 0.48 it sat 22 px off the line and simply
    // looked wrong.) Kept inside the path's room so it can never land on the buttons either side.
    property real mirrorCenterX: width / 2
    ToggleButton {
        id: mirrorBtn
        objectName: "toolbar-mirror"
        visible: bar.split
        z: 2
        anchors.verticalCenter: parent.verticalCenter
        x: {
            const want = bar.mirrorCenterX - width / 2
            const lo = row.x + crumb.x, hi = row.x + crumb.x + crumb.width - width
            return hi >= lo ? Math.max(lo, Math.min(hi, want)) : want
        }
        flat: true; iconSize: 20
        icon: "mirror"; active: bar.mirror
        tip: "Mirror… (Ctrl+M)"
        onClicked: bar.toggleMirror()
    }
    // Side by side goes with them (owner, 2026-09-21): it is a server and the folder it is kept
    // beside, so it is offered only while a server is open — and while it is on, so there is
    // always a way back to one pane. On, it stands left of Mirror; off, where the path ends,
    // which gives it up the room (`sbsSlot`).
    ToggleButton {
        id: sideBySideBtn
        objectName: "side-by-side"
        visible: (bar.remoteOpen || bar.split) && bar.width >= 360
        z: 2
        anchors.verticalCenter: parent.verticalCenter
        x: bar.split ? mirrorBtn.x - width - 4 : row.x + sbsSlot.x
        // Never flat: lit — box and all — while it is on, as it always was.
        iconSize: bar.split ? 20 : 16
        icon: "split"; active: bar.split
        tip: (bar.split ? "Back to one pane" : "Side by Side") + " (Ctrl+4)"
        onClicked: bar.toggleSplit()
    }
    // Beside it while a pane is on a server: let go of that server. The way to it was the
    // sidebar's menu, two clicks and a panel away from where the server is being looked at.
    ToggleButton {
        id: disconnectBtn
        objectName: "toolbar-disconnect"
        visible: bar.split && bar.remoteHost !== "" && !bar.mirror
        z: 2
        anchors.verticalCenter: parent.verticalCenter
        x: mirrorBtn.x + mirrorBtn.width + 4
        flat: true; iconSize: 20
        icon: "disconnect"
        tip: "Disconnect from " + bar.remoteHost
        onClicked: bar.disconnect()
    }
    Row {
        id: row
        anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8; spacing: 4
        // The buttons keep their natural size and the path takes the rest, so the toolbar fits
        // the window instead of spilling over the sidebar. The view button is the first to go
        // when the window is too narrow for it.
        readonly property int fixedCount: 3 + (viewButton.visible ? 1 : 0) + (sbsSlot.visible ? 1 : 0)
        readonly property int fixedWidth: sidebarBtn.width + (viewButton.visible ? viewButton.width : 0)
            + (sbsSlot.visible ? sbsSlot.width : 0) + searchBtn.width + gearBtn.width
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
            opacity: bar.pathShown ? 1 : 0
            enabled: bar.pathShown
            width: row.crumbWidth
            uri: bar.crumbUri !== "" ? bar.crumbUri : (bar.crumbPane ? bar.crumbPane.uri : ""); home: bar.home
            onNavigate: uri => bar.navigate(uri)
            onPathMenu: bar.pathMenu()
        }
        ToggleButton {
            id: searchBtn
            anchors.verticalCenter: parent.verticalCenter
            icon: "search"; tip: "Search everywhere (Ctrl+Shift+F)"
            onClicked: bar.toggleSearch()
        }
        // Where the side-by-side button stands while there is one pane and a server in it: the
        // button itself floats (it moves to Mirror's side when on), and this keeps its room.
        Item { id: sbsSlot; visible: sideBySideBtn.visible && !bar.split; width: sideBySideBtn.width; height: 1 }
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
                Icon { name: "gear"; color: Kiki.Theme.chrome; anchors.verticalCenter: parent.verticalCenter }
                Icon { name: "chev-d"; size: 10; color: Kiki.Theme.chrome; anchors.verticalCenter: parent.verticalCenter }
            }
            MouseArea { id: gearHover; anchors.fill: parent; hoverEnabled: true; onClicked: bar.settings() }
        }
    }
}
