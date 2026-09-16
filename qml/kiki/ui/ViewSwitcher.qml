import QtQuick
import ".." as Kiki

Rectangle {
    id: sw
    property string view: "list"
    signal changed(string view)
    width: row.width + 4; height: 34; radius: 2
    color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line
    Row {
        id: row; anchors.centerIn: parent; spacing: 2
        Repeater {
            model: [{ v: "icon", i: "grid" }, { v: "list", i: "list" }, { v: "columns", i: "columns" }]
            delegate: Rectangle {
                required property var modelData
                width: 34; height: 28; radius: 2
                color: sw.view === modelData.v ? Kiki.Theme.surface : "transparent"
                Icon { anchors.centerIn: parent; name: modelData.i; color: sw.view === modelData.v ? Kiki.Theme.accent : Kiki.Theme.muted }
                MouseArea { anchors.fill: parent; onClicked: sw.changed(modelData.v) }
            }
        }
    }
}
