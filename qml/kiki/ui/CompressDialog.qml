import QtQuick
import Quickshell
import ".." as Kiki

// Compress…: archive name, format, into the current folder.
Rectangle {
    id: dlg
    property var items: []            // URIs
    property string dest: ""          // folder URI
    signal submit(string archiveUri, string format)
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 90
    property var formats: ["zip", "tar.gz", "tar.zst", "tar.xz", "tar.bz2", "tar", "7z"]
    property int formatIndex: 0
    function open(uris, folder) {
        items = uris; dest = folder; visible = true
        const base = uris.length === 1 ? decodeURIComponent(uris[0].split("/").pop()).replace(/\.[^.]+$/, "") : "archive"
        nameInput.text = base; nameInput.forceActiveFocus()
        // Selected all, but with the cursor at the front, so a long name shows its beginning
        // rather than scrolling to the tail. Typing still replaces the lot.
        nameInput.select(nameInput.length, 0)
    }
    MouseArea { anchors.fill: parent }
    Rectangle {
        anchors.centerIn: parent; width: 460; height: 210; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            anchors.fill: parent; anchors.margins: 20; spacing: 14
            Text { text: Kiki.T.tr("compress.title", { n: dlg.items.length }); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            Row {
                spacing: 8; width: parent.width
                Rectangle {
                    // A long name has to stay in its box: an unclipped TextInput draws the whole
                    // string, straight out through the side of the dialog.
                    width: parent.width - 150; height: 32; radius: 2; clip: true
                    color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
                    TextInput { id: nameInput; anchors.fill: parent; anchors.margins: 8; clip: true; verticalAlignment: TextInput.AlignVCenter; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; onAccepted: dlg.go() }
                }
                Rectangle {
                    width: 142; height: 32; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
                    Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: "." + dlg.formats[dlg.formatIndex]; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    Icon { anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; name: "chev-d"; size: 12; color: Kiki.Theme.muted }
                    MouseArea { anchors.fill: parent; onClicked: dlg.formatIndex = (dlg.formatIndex + 1) % dlg.formats.length }
                }
            }
            // A deep folder is elided in the middle: the start and the leaf are what tell you where
            // the archive is going.
            Text {
                width: parent.width; elide: Text.ElideMiddle
                text: Kiki.T.tr("compress.into", { folder: Kiki.Format.display(dlg.dest, Quickshell.env("HOME")) })
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
            Row {
                spacing: 8; anchors.right: parent.right
                Button { text: Kiki.T.tr("common.cancel"); onClicked: dlg.visible = false }
                Button { text: Kiki.T.tr("compress.go"); primary: true; onClicked: dlg.go() }
            }
        }
    }
    function go() {
        const name = nameInput.text.trim(); if (!name) return
        visible = false
        submit(dest.replace(/\/+$/, "") + "/" + encodeURIComponent(name + "." + formats[formatIndex]), formats[formatIndex])
    }
    Keys.onEscapePressed: visible = false
}
