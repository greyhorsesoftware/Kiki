import QtQuick
import Quickshell
import "kiki" as Kiki

// Plan 01 shell skeleton: one window, one list, proving the window cache path.
// Plan 02 replaces the body with the sidebar, toolbar and views.
ShellRoot {
    FloatingWindow {
        id: win
        title: "kiki"
        implicitWidth: 1200
        implicitHeight: 760
        color: "#1a1b26"

        Kiki.WindowCache {
            id: listing
            Component.onCompleted: open(Quickshell.env("KIKI_START") || ("file://" + Quickshell.env("HOME")))
        }

        Column {
            anchors.fill: parent
            Rectangle {
                width: parent.width; height: 48; color: "#1a1b26"
                Text {
                    anchors.verticalCenter: parent.verticalCenter; x: 14
                    text: listing.uri + "   " + listing.count + (listing.done ? "" : " …") + (listing.error ? "   " + listing.error : "")
                    color: "#c0caf5"; font.family: "Cascadia Mono"; font.pixelSize: 13
                }
                Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: "#292e42" }
            }
            ListView {
                id: view
                width: parent.width; height: parent.height - 48
                clip: true
                reuseItems: true
                cacheBuffer: 28 * 40
                model: listing.count
                onContentYChanged: {
                    const first = Math.max(0, Math.floor(contentY / 28))
                    listing.setViewport(first, Math.ceil(height / 28) + 1)
                }
                Connections {
                    target: listing
                    function onRowsUpdated(first, n) { view.forceLayout() }
                    function onReset() { view.forceLayout() }
                }
                delegate: Rectangle {
                    required property int index
                    property var row: listing.row(index)
                    width: ListView.view.width; height: 28
                    color: "transparent"
                    Connections { target: listing; function onRowsUpdated(first, n) { if (index >= first && index < first + n) row = listing.row(index) } }
                    Row {
                        anchors.verticalCenter: parent.verticalCenter; x: 12; spacing: 12
                        Text { width: 520; elide: Text.ElideRight; text: row ? row.name : "…"; color: row ? (row.isDir ? "#7aa2f7" : "#c0caf5") : "#565f89"; font.family: "Cascadia Mono"; font.pixelSize: 13 }
                        Text { width: 100; horizontalAlignment: Text.AlignRight; text: row && row.meta ? row.meta.size : ""; color: "#565f89"; font.family: "Cascadia Mono"; font.pixelSize: 12 }
                        Text { text: row && row.meta ? new Date(row.meta.mtime).toLocaleString(Qt.locale(), "d MMM yyyy HH:mm") : ""; color: "#565f89"; font.family: "Cascadia Mono"; font.pixelSize: 12 }
                    }
                }
            }
        }
    }
}
