import QtQuick
import ".." as Kiki

// The keymap, on "?". Grouped the way the keys are meant to be learned: moving around first.
Rectangle {
    id: ov
    signal closed()
    anchors.fill: parent
    visible: false
    color: Qt.rgba(0, 0, 0, 0.5)
    z: 95

    function open() { visible = true; panel.forceActiveFocus() }
    function close() { visible = false; closed() }

    readonly property var groups: [
        { title: "Move around", keys: [
            ["↑ ↓", "move the selection"],
            ["←", "up a level (parent column in columns view)"],
            ["→", "open the folder (child column in columns view)"],
            ["Enter", "open"],
            ["Backspace", "up a level"],
            ["Alt+←  Alt+→", "back / forward"],
            ["Home  End", "first / last"],
            ["PgUp  PgDn", "page"],
            ["Tab", "other pane (mirror view)"],
            ["Ctrl+B", "focus favorites"],
            ["Ctrl+Shift+B", "show / hide favorites"],
        ] },
        { title: "Find", keys: [
            ["/   Ctrl+F", "filter this folder"],
            ["Ctrl+Shift+F", "search everywhere"],
            ["Ctrl+L", "type a path"],
            ["type a name", "jump to it (off with Vim keys)"],
        ] },
        { title: "View", keys: [
            ["Ctrl+1 .. Ctrl+4", "icon / list / columns / mirror"],
            ["Ctrl+H", "hidden files"],
            ["Ctrl+I", "info panel"],
            ["Ctrl+,", "settings"],
            ["F5", "refresh"],
        ] },
        { title: "Files", keys: [
            ["F2", "rename"],
            ["F4", "edit"],
            ["Del", "move to trash"],
            ["Shift+Del", "delete for good"],
            ["Ctrl+C  Ctrl+X  Ctrl+V", "copy / cut / paste"],
            ["Ctrl+Shift+C", "copy path"],
            ["Ctrl+Shift+N", "new folder"],
            ["Ctrl+Z  Ctrl+Shift+Z", "undo / redo"],
            ["Alt+Enter", "open with the default tool"],
            ["Alt+Shift+Enter", "open with…"],
            ["Alt+S", "share"],
            ["Alt+Q", "ask Jarvis"],
        ] },
    ]

    MouseArea { anchors.fill: parent; onClicked: ov.close() }

    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: Math.min(820, parent.width - 32)
        height: Math.min(560, parent.height - 32)
        color: Kiki.Theme.bg; radius: 3; border.width: 1; border.color: Kiki.Theme.line
        clip: true
        focus: true
        Keys.onEscapePressed: ov.close()
        MouseArea { anchors.fill: parent }

        Item {
            id: head
            width: parent.width; height: 36
            Text { x: 16; anchors.verticalCenter: parent.verticalCenter; text: "Keyboard"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 14; font.bold: true }
            ToggleButton { anchors.right: parent.right; anchors.rightMargin: 6; anchors.verticalCenter: parent.verticalCenter; icon: "x"; onClicked: ov.close() }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
        }
        Flickable {
            anchors.top: head.bottom; width: parent.width; height: parent.height - head.height
            contentWidth: width; contentHeight: body.height + 32; clip: true
            boundsBehavior: Flickable.StopAtBounds
            NaturalScroll { }
            Column {
                id: body
                x: 16; y: 16; width: parent.width - 32; spacing: 18
                Repeater {
                    model: ov.groups
                    delegate: Column {
                        required property var modelData
                        width: parent.width; spacing: 6
                        Text { text: modelData.title.toUpperCase(); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true; font.letterSpacing: 1 }
                        Repeater {
                            model: modelData.keys
                            delegate: Row {
                                required property var modelData
                                width: parent.width; spacing: 14; height: 22
                                Text { width: 190; horizontalAlignment: Text.AlignRight; text: modelData[0]; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                                Text { width: parent.width - 204; elide: Text.ElideRight; text: modelData[1]; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                            }
                        }
                    }
                }
            }
        }
    }
}
