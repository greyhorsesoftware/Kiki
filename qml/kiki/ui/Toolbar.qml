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
    signal toggleInspector()
    signal toggleSplit()
    property alias search: search
    property var locations: []
    property var repo: null
    property string openInDefault: ""
    signal search(string text, string scope)
    signal scopeMenu()
    signal openIn(string id)
    signal openInMenu()
    property alias breadcrumb: crumb
    height: Kiki.Theme.toolbarHeight
    color: Kiki.Theme.bg
    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }

    Row {
        anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 8
        Row {
            spacing: 2; anchors.verticalCenter: parent.verticalCenter
            Repeater {
                model: [{ i: "arr-l", f: "back" }, { i: "arr-r", f: "forward" }]
                delegate: Rectangle {
                    required property var modelData
                    property bool enabled: modelData.f === "back" ? bar.pane.canBack() : bar.pane.canForward()
                    width: 28; height: 28; radius: 2; color: "transparent"
                    Icon { anchors.centerIn: parent; name: modelData.i; color: parent.enabled ? Kiki.Theme.fgDim : Kiki.Theme.gutter }
                    MouseArea { anchors.fill: parent; onClicked: modelData.f === "back" ? bar.pane.back() : bar.pane.forward() }
                }
            }
        }
        Breadcrumb {
            id: crumb
            repo: bar.repo
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 58 - 8 - 260 - 8 - 112 - 8 - 34 - 8 - 34 - 16 - (bar.split ? 42 : 0) - (bar.openInDefault !== "" ? openInWidth : 0)
            property int openInWidth: 150
            uri: bar.pane.uri; home: bar.home
            onNavigate: uri => bar.pane.open(uri)
        }
        SearchBox {
            id: search
            anchors.verticalCenter: parent.verticalCenter
            placeholder: "Search " + (Kiki.Format.crumbs(bar.pane.uri, bar.home).slice(-1)[0] || "")
            scopes: bar.locations.map(l => ({ id: l.name, label: l.name }))
            onChanged: text => bar.search(text, search.scope)
            onScopeMenu: bar.scopeMenu()
        }
        SplitButton { anchors.verticalCenter: parent.verticalCenter; label: bar.openInDefault; icon: "terminal"; enabled: bar.openInDefault !== ""; onClicked: bar.openIn(""); onMenu: bar.openInMenu() }
        ViewSwitcher { anchors.verticalCenter: parent.verticalCenter; view: bar.pane.view; onChanged: v => bar.pane.view = v }
        ToggleButton { anchors.verticalCenter: parent.verticalCenter; icon: "split"; active: bar.split; onClicked: bar.toggleSplit() }
        ToggleButton { anchors.verticalCenter: parent.verticalCenter; icon: "info"; active: bar.inspector; onClicked: bar.toggleInspector() }
        ToggleButton { visible: bar.split; anchors.verticalCenter: parent.verticalCenter; icon: "mirror"; active: bar.mirror; tip: "Mirror local and remote"; onClicked: bar.toggleMirror() }
    }
}
