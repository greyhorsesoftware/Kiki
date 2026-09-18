import QtQuick
import ".." as Kiki

// About kiki: the panel you get from the gear menu. Laid out the way macOS does it — the mark,
// the name, the version and build under it, and who made it at the foot.
Rectangle {
    id: dlg
    anchors.fill: parent
    visible: false
    color: Qt.rgba(0, 0, 0, 0.5)
    z: 96

    property var about: ({})

    function open() {
        Kiki.Daemon.request("About", {}, ok => { if (ok) dlg.about = ok })
        visible = true
        panel.forceActiveFocus()
    }
    function close() { visible = false; closed() }
    signal closed()

    MouseArea { anchors.fill: parent; onClicked: dlg.close() }

    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: 380; height: col.height + 56
        radius: 12
        color: Kiki.Theme.bg
        border.width: 1; border.color: Kiki.Theme.line
        focus: dlg.visible
        Keys.onEscapePressed: dlg.close()
        Keys.onReturnPressed: dlg.close()
        MouseArea { anchors.fill: parent }        // clicks inside must not reach the backdrop

        ToggleButton {
            anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 6
            icon: "x"; tip: "Close"; onClicked: dlg.close()
        }

        Column {
            id: col
            y: 28
            width: parent.width
            spacing: 6

            // The mark: the wireframe cat, kiki herself.
            AppMark {
                objectName: "about-icon"
                anchors.horizontalCenter: parent.horizontalCenter
                width: 112; height: 112
            }
            Item { width: 1; height: 8 }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "kiki"; color: Kiki.Theme.fg
                font.family: Kiki.Theme.mono; font.pixelSize: 26; font.bold: true
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "A fast file manager for Omarchy"
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
            Item { width: 1; height: 10 }
            Text {
                objectName: "about-version"
                anchors.horizontalCenter: parent.horizontalCenter
                text: "Version " + (dlg.about.version || "—") + " (" + (dlg.about.build || "dev") + ")"
                color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
            Item { width: 1; height: 16 }
            Rectangle { anchors.horizontalCenter: parent.horizontalCenter; width: parent.width - 80; height: 1; color: Kiki.Theme.line }
            Item { width: 1; height: 14 }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "Crafted by Grey Horse Softworks"
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
        }
    }
}
