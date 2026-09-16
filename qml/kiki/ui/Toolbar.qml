import QtQuick
import ".." as Kiki

Rectangle {
    id: bar
    property Kiki.Pane pane
    property string home: ""
    property bool inspector: false
    property bool split: false
    signal toggleInspector()
    signal toggleSplit()
    property alias search: search
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
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - 58 - 8 - 260 - 8 - 112 - 8 - 34 - 8 - 34 - 16
            uri: bar.pane.uri; home: bar.home
            onNavigate: uri => bar.pane.open(uri)
        }
        SearchBox {
            id: search
            anchors.verticalCenter: parent.verticalCenter
            placeholder: "Search " + (Kiki.Format.crumbs(bar.pane.uri, bar.home).slice(-1)[0] || "")
            onChanged: text => bar.pane.setFilter(text)
        }
        ViewSwitcher { anchors.verticalCenter: parent.verticalCenter; view: bar.pane.view; onChanged: v => bar.pane.view = v }
        ToggleButton { anchors.verticalCenter: parent.verticalCenter; icon: "split"; active: bar.split; onClicked: bar.toggleSplit() }
        ToggleButton { anchors.verticalCenter: parent.verticalCenter; icon: "info"; active: bar.inspector; onClicked: bar.toggleInspector() }
    }
}
