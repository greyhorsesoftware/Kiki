import QtQuick
import ".." as Kiki

// Filters the current listing as you type (plan 12 adds scopes).
Rectangle {
    id: box
    property string placeholder: Kiki.T.tr("search.box")
    property alias text: input.text
    property bool active: input.activeFocus
    property string scope: "folder"       // folder | everywhere | <location name>
    property var scopes: []                 // [{ id, label }] beyond folder/everywhere
    signal scopeMenu()
    function scopeLabel() { return scope === "folder" ? "" : (scope === "everywhere" ? "Everywhere" : scope) }
    signal changed(string text)
    signal accepted()
    signal escaped()
    signal moveResult(int delta)
    width: 260; height: 30; radius: 2; clip: true
    color: Kiki.Theme.bgDark; border.width: 1; border.color: active ? Kiki.Theme.accent : Kiki.Theme.line
    function focus() { input.forceActiveFocus() }
    function clear() { input.text = ""; scope = "folder"; box.changed("") }
    function cycleScope() { const all = ["folder", "everywhere"].concat(scopes.map(s => s.id)); scope = all[(all.indexOf(scope) + 1) % all.length]; box.changed(input.text) }
    // Typed prefixes: "everywhere:" / "all:" / "<location>:" at the start of the field become the scope chip.
    function absorbPrefix() {
        const m = input.text.match(/^([A-Za-z0-9_.-]+):/)
        if (!m) return
        const p = m[1].toLowerCase()
        const loc = scopes.find(s => s.id.toLowerCase() === p)
        if (p === "everywhere" || p === "all") scope = "everywhere"; else if (loc) scope = loc.id; else return
        input.text = input.text.slice(m[0].length)
    }
    Row {
        anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 8; spacing: 8
        Icon { name: "search"; size: 14; color: box.active ? Kiki.Theme.accent : Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
        // scope chip, or a chevron that opens the scope menu
        Rectangle {
            visible: box.scope !== "folder"; height: 20; width: chipRow.width + 12; radius: 2; color: Kiki.Theme.surface; anchors.verticalCenter: parent.verticalCenter
            Row { id: chipRow; anchors.centerIn: parent; spacing: 2
                Text { text: box.scopeLabel(); color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
                Text { text: ":"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
            MouseArea { anchors.fill: parent; onClicked: box.scopeMenu() }
        }
        Item { visible: box.scope === "folder" && box.active; width: 14; height: 20; anchors.verticalCenter: parent.verticalCenter
            Icon { anchors.centerIn: parent; name: "chev-d"; size: 10; color: Kiki.Theme.muted }
            MouseArea { anchors.fill: parent; anchors.margins: -4; onClicked: box.scopeMenu() } }
        TextInput {
            id: input
            width: Math.max(0, parent.width - 60); height: parent.height; verticalAlignment: TextInput.AlignVCenter
            color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
            onTextChanged: { box.absorbPrefix(); debounce.restart() }
            onAccepted: box.accepted()
            Keys.onEscapePressed: { if (text.length || box.scope !== "folder") { text = ""; box.scope = "folder"; box.changed("") } else box.escaped() }
            Keys.onTabPressed: box.cycleScope()
            Keys.onPressed: event => { if (event.key === Qt.Key_Backspace && !text.length && box.scope !== "folder") { box.scope = "folder"; box.changed(""); event.accepted = true } if (event.key === Qt.Key_Down) { box.moveResult(1); event.accepted = true } if (event.key === Qt.Key_Up) { box.moveResult(-1); event.accepted = true } }
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
