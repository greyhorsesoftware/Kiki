import QtQuick
import ".." as Kiki

// Every shortcut, and the place to change one: click a row, press the keys you want, done.
// Esc while recording puts the row back as it was.
Rectangle {
    id: win
    anchors.fill: parent
    visible: false
    color: Qt.rgba(0, 0, 0, 0.5)
    z: 96

    property var keymap: null
    /// The action being recorded, "" when nothing is.
    property string recording: ""
    /// Set when the keys just pressed are already spoken for.
    property string clash: ""

    function open() { visible = true; recording = ""; clash = ""; panel.forceActiveFocus() }
    function close() { visible = false; recording = ""; closed() }
    signal closed()

    MouseArea { anchors.fill: parent; onClicked: win.close() }

    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: Math.min(680, parent.width - 48)
        height: Math.min(720, parent.height - 64)
        radius: 10
        color: Kiki.Theme.bg
        border.width: 1; border.color: Kiki.Theme.line
        focus: win.visible
        MouseArea { anchors.fill: parent }

        // While recording, this takes the keys; otherwise Esc closes the window.
        Keys.onPressed: event => {
            if (!win.recording) {
                if (event.key === Qt.Key_Escape) { win.close(); event.accepted = true }
                return
            }
            event.accepted = true
            if (event.key === Qt.Key_Escape) { win.recording = ""; win.clash = ""; return }
            // A modifier on its own is not a shortcut, it is the start of one.
            if (event.key === Qt.Key_Control || event.key === Qt.Key_Shift || event.key === Qt.Key_Alt
                || event.key === Qt.Key_Meta || event.key === Qt.Key_Super_L || event.key === Qt.Key_Super_R) return
            const chord = win.keymap.encode(event.key, event.modifiers, event.text)
            if (!chord) return
            const taken = win.keymap.idForChord(chord, win.recording)
            if (taken) { win.clash = taken; return }
            win.keymap.rebind(win.recording, chord)
            win.recording = ""; win.clash = ""
        }

        Column {
            anchors.fill: parent; anchors.margins: 18; spacing: 12

            Item {
                width: parent.width; height: 28
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: Kiki.T.tr("keys.title"); color: Kiki.Theme.fg
                    font.family: Kiki.Theme.mono; font.pixelSize: 16; font.bold: true
                }
                Row {
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; spacing: 8
                    Button { objectName: "keys-reset-all"; height: 26; text: Kiki.T.tr("keys.resetAll"); onClicked: win.keymap.resetAll() }
                    ToggleButton { icon: "x"; tip: Kiki.T.tr("keys.close"); onClicked: win.close() }
                }
            }
            Text {
                width: parent.width
                text: win.recording
                    ? (win.clash ? Kiki.T.tr("keys.clash", { action: win.keymap.find(win.clash).label.toLowerCase() })
                                 : Kiki.T.tr("keys.press", { action: win.keymap.find(win.recording).label.toLowerCase() }))
                    : Kiki.T.tr("keys.intro")
                wrapMode: Text.WordWrap
                color: win.clash ? Kiki.Theme.danger : (win.recording ? Kiki.Theme.accent : Kiki.Theme.muted)
                font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
            Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }

            Flickable {
                width: parent.width; height: parent.height - 92
                contentWidth: width; contentHeight: rows.height
                clip: true; boundsBehavior: Flickable.StopAtBounds
                NaturalScroll { }
                Column {
                    id: rows
                    width: parent.width; spacing: 2
                    Repeater {
                        model: win.keymap ? win.keymap.groups : []
                        delegate: Column {
                            required property var modelData
                            width: rows.width; spacing: 2
                            Item { width: 1; height: 10 }
                            Text {
                                text: win.keymap.groupLabel(modelData).toUpperCase(); color: Kiki.Theme.muted
                                font.family: Kiki.Theme.mono; font.pixelSize: 10; font.bold: true; font.letterSpacing: 1
                            }
                            Repeater {
                                model: win.keymap.actions.filter(a => a.group === modelData)
                                delegate: Rectangle {
                                    id: row
                                    required property var modelData
                                    objectName: "key-" + modelData.id
                                    width: rows.width; height: 30; radius: 6
                                    readonly property bool active: win.recording === modelData.id
                                    color: active ? Qt.rgba(Kiki.Theme.accent.r, Kiki.Theme.accent.g, Kiki.Theme.accent.b, 0.16)
                                                  : (hover.containsMouse ? Qt.rgba(1, 1, 1, 0.03) : "transparent")
                                    Text {
                                        x: 10; anchors.verticalCenter: parent.verticalCenter
                                        width: parent.width - 220; elide: Text.ElideRight
                                        text: modelData.label; color: Kiki.Theme.fgDim
                                        font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                                    }
                                    Row {
                                        anchors.right: parent.right; anchors.rightMargin: 10
                                        anchors.verticalCenter: parent.verticalCenter; spacing: 8
                                        Text {
                                            visible: win.keymap.isCustom(modelData.id)
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: Kiki.T.tr("keys.changed"); color: Kiki.Theme.accent
                                            font.family: Kiki.Theme.mono; font.pixelSize: 10
                                        }
                                        Rectangle {
                                            anchors.verticalCenter: parent.verticalCenter
                                            height: 20; width: chordText.implicitWidth + 14; radius: 3
                                            color: row.active ? Kiki.Theme.accent : "transparent"
                                            border.width: 1; border.color: row.active ? Kiki.Theme.accent : Kiki.Theme.gutter
                                            Text {
                                                id: chordText
                                                anchors.centerIn: parent
                                                text: row.active ? "…" : win.keymap.chordFor(modelData.id)
                                                color: row.active ? Kiki.Theme.bg : Kiki.Theme.fgDim
                                                font.family: Kiki.Theme.mono; font.pixelSize: 11
                                            }
                                        }
                                        ToggleButton {
                                            visible: win.keymap.isCustom(modelData.id)
                                            icon: "x"; tip: Kiki.T.tr("keys.backTo", { key: modelData.def })
                                            onClicked: win.keymap.reset(modelData.id)
                                        }
                                    }
                                    MouseArea {
                                        id: hover
                                        anchors.fill: parent; hoverEnabled: true
                                        onClicked: { win.clash = ""; win.recording = win.recording === modelData.id ? "" : modelData.id; panel.forceActiveFocus() }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
