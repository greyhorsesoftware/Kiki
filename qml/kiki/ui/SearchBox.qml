import QtQuick
import ".." as Kiki

// Filters the current listing as you type (plan 12 adds scopes).
Rectangle {
    id: box
    property string placeholder: "Search"
    property alias text: input.text
    property bool active: input.activeFocus
    signal changed(string text)
    signal accepted()
    signal escaped()
    width: 260; height: 30; radius: 2
    color: Kiki.Theme.bgDark; border.width: 1; border.color: active ? Kiki.Theme.accent : Kiki.Theme.line
    function focus() { input.forceActiveFocus() }
    function clear() { input.text = ""; box.changed("") }
    Row {
        anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 8; spacing: 8
        Icon { name: "search"; size: 14; color: box.active ? Kiki.Theme.accent : Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
        TextInput {
            id: input
            width: parent.width - 60; height: parent.height; verticalAlignment: TextInput.AlignVCenter
            color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
            onTextChanged: debounce.restart()
            onAccepted: box.accepted()
            Keys.onEscapePressed: { if (text.length) { text = ""; box.changed("") } else box.escaped() }
            Text { visible: !input.text.length && !input.activeFocus; text: box.placeholder; color: Kiki.Theme.muted; font: input.font; anchors.verticalCenter: parent.verticalCenter }
        }
        Rectangle {
            visible: !input.activeFocus && !input.text.length
            width: 16; height: 16; radius: 2; border.width: 1; border.color: Kiki.Theme.gutter; color: "transparent"; anchors.verticalCenter: parent.verticalCenter
            Text { anchors.centerIn: parent; text: "/"; color: Kiki.Theme.gutter; font.family: Kiki.Theme.mono; font.pixelSize: 10 }
        }
    }
    Timer { id: debounce; interval: Kiki.Settings.timers.searchDebounceMs; onTriggered: box.changed(input.text) }
}
