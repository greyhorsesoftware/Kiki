import QtQuick
import ".." as Kiki
import "../ui" as UI

// Global search (plan 12): a panel over the window rather than a replacement for the pane, so
// closing it leaves the folder, selection and scroll exactly as they were.
Rectangle {
    id: ov
    property Kiki.WindowCache results
    property var locations: []
    property string home: ""
    property string indexInfo: ""
    property string scope: "everywhere"
    property alias text: input.text
    signal search(string text, string scope)
    signal openUri(string uri)
    signal revealUri(string uri)
    signal closed()

    anchors.fill: parent
    visible: false
    color: Qt.rgba(0, 0, 0, 0.5)
    z: 94

    function open(seed) {
        visible = true
        if (seed !== undefined && seed !== input.text) input.text = seed
        input.forceActiveFocus(); input.selectAll()
        if (input.text) ov.search(input.text, ov.scope)
    }
    function close() { visible = false; closed() }

    MouseArea { anchors.fill: parent; onClicked: ov.close() }

    Rectangle {
        id: panel
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.round(Math.min(80, parent.height * 0.1))
        width: Math.min(760, parent.width - 32)
        height: Math.min(520, parent.height - 2 * y)
        color: Kiki.Theme.bg; radius: 3; border.width: 1; border.color: Kiki.Theme.line
        clip: true
        MouseArea { anchors.fill: parent }

        Item {
            id: field
            width: parent.width; height: 46
            UI.Icon { x: 16; anchors.verticalCenter: parent.verticalCenter; name: "search"; size: 15; color: Kiki.Theme.muted }
            TextInput {
                id: input
                x: 44; width: parent.width - 60; height: parent.height
                verticalAlignment: TextInput.AlignVCenter; clip: true
                color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15
                selectionColor: Kiki.Theme.accent
                onTextChanged: debounce.restart()
                onAccepted: list.activate()
                Keys.onEscapePressed: ov.close()
                Keys.onDownPressed: list.move(1)
                Keys.onUpPressed: list.move(-1)
                Text {
                    visible: !input.text.length; anchors.verticalCenter: parent.verticalCenter
                    text: Kiki.T.tr("search.placeholder"); color: Kiki.Theme.muted; font: input.font
                }
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
        }
        Row {
            id: scopes
            x: 12; y: field.height + 8; spacing: 6; height: 24
            Repeater {
                model: [{ id: "everywhere", label: Kiki.T.tr("search.everywhere") }].concat(ov.locations.map(l => ({ id: l.name, label: l.name })))
                delegate: Rectangle {
                    required property var modelData
                    height: 24; radius: 12; width: chip.implicitWidth + 20
                    color: ov.scope === modelData.id ? Kiki.Theme.accent : Kiki.Theme.surface
                    Text {
                        id: chip; anchors.centerIn: parent; text: modelData.label
                        color: ov.scope === modelData.id ? Kiki.Theme.bg : Kiki.Theme.fgDim
                        font.family: Kiki.Theme.mono; font.pixelSize: 11
                    }
                    MouseArea { anchors.fill: parent; onClicked: { ov.scope = modelData.id; if (input.text) ov.search(input.text, ov.scope) } }
                }
            }
        }
        SearchResults {
            id: list
            y: scopes.y + scopes.height + 8
            width: parent.width; height: Math.max(0, parent.height - y)
            results: ov.results; query: input.text; home: ov.home; indexInfo: ov.indexInfo
            scopeLabel: ov.scope === "everywhere" ? Kiki.T.tr("search.everywhere") : ov.scope
            onOpen: uri => { ov.close(); ov.openUri(uri) }
            onReveal: uri => { ov.close(); ov.revealUri(uri) }
        }
        Timer { id: debounce; interval: Kiki.Settings.timers.searchDebounceMs; onTriggered: ov.search(input.text, ov.scope) }
    }
}
