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
    /// The info panel is up. The button beside Search lights while it is, and toggles it.
    property bool inspector: false
    /// Columns view has an info column of its own and the window's panel never shows there, and
    /// with nothing selected in list, icon or gallery view there is nothing to show: the button
    /// dims either way.
    property bool inspectorAvailable: true
    signal toggleInspector()
    /// Too narrow for the buttons: they fold into one, a hamburger, whose menu carries every
    /// one of them (owner, 2026-09-22). `hamburger(pos)` asks the window for that menu.
    signal hamburger(var button)
    /// The path keeps at least this much, or the buttons fold. Worked out from the buttons'
    /// fixed sizes, never from what is showing, so folding cannot feed back into itself.
    readonly property int pathMin: 180
    readonly property bool compact: width - 16 - (34 + 34 + 34 + 52 + 48 + 34) - 4 * 6 < pathMin
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
        tip: Kiki.T.tr("toolbar.mirrorTip")
        onClicked: bar.toggleMirror()
    }
    // Side by side goes with them (owner, 2026-09-21): it is a server and the folder it is kept
    // beside, so it is offered only while a server is open — and while it is on, so there is
    // always a way back to one pane. On, it stands left of Mirror; off, where the path ends,
    // which gives it up the room (`sbsSlot`).
    ToggleButton {
        id: sideBySideBtn
        objectName: "side-by-side"
        visible: (bar.remoteOpen || bar.split) && !(bar.compact && !bar.split)
        z: 2
        anchors.verticalCenter: parent.verticalCenter
        x: bar.split ? mirrorBtn.x - width - 4 : row.x + sbsSlot.x
        // Never flat: lit — box and all — while it is on, as it always was.
        iconSize: bar.split ? 20 : 16
        icon: "split"; active: bar.split
        tip: bar.split ? Kiki.T.tr("toolbar.onePane") : Kiki.T.tr("toolbar.sideBySide")
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
        tip: Kiki.T.tr("toolbar.disconnectTip", { host: bar.remoteHost })
        onClicked: bar.disconnect()
    }
    Row {
        id: row
        anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 8; spacing: 4
        // The buttons keep their natural size and the path takes the rest, so the toolbar fits
        // the window instead of spilling over the sidebar. The view button is the first to go
        // when the window is too narrow for it.
        readonly property int fixedCount: bar.compact ? 2 : 4 + (viewButton.visible ? 1 : 0) + (sbsSlot.visible ? 1 : 0)
        readonly property int fixedWidth: sidebarBtn.width + (bar.compact ? menuBtn.width
            : infoBtn.width + (viewButton.visible ? viewButton.width : 0) + (sbsSlot.visible ? sbsSlot.width : 0) + gearBtn.width)
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
        // Search everywhere lives on the rail (0.1.1); the magnifier that stood here is gone.
        // The info panel, opened and closed (owner, 2026-09-22). The same toggle as Ctrl+I and
        // the menu's Get info; lit while the panel is up.
        ToggleButton {
            id: infoBtn
            objectName: "toolbar-info"
            visible: !bar.compact
            anchors.verticalCenter: parent.verticalCenter
            icon: "info"; active: bar.inspector && bar.inspectorAvailable
            enabled: bar.inspectorAvailable
            tip: bar.inspectorAvailable ? (bar.inspector ? Kiki.T.tr("toolbar.hideInfo") : Kiki.T.tr("toolbar.showInfo")) : ""
            onClicked: bar.toggleInspector()
        }
        // Where the side-by-side button stands while there is one pane and a server in it: the
        // button itself floats (it moves to Mirror's side when on), and this keeps its room.
        Item { id: sbsSlot; visible: sideBySideBtn.visible && !bar.split && !bar.compact; width: sideBySideBtn.width; height: 1 }
        ViewSwitcher { id: viewButton; visible: !bar.compact; anchors.verticalCenter: parent.verticalCenter; view: bar.pane.view; onMenu: bar.viewMenu() }
        // Everything above, folded into one menu when there is no room for the buttons.
        ToggleButton {
            id: menuBtn
            objectName: "toolbar-menu"
            visible: bar.compact
            anchors.verticalCenter: parent.verticalCenter
            icon: "menu"; tip: Kiki.T.tr("toolbar.menu")
            onClicked: bar.hamburger(menuBtn)
        }
        // A menu, not a button: the chevron says so, the way the view switcher does.
        Rectangle {
            id: gearBtn
            objectName: "gear"
            visible: !bar.compact
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
