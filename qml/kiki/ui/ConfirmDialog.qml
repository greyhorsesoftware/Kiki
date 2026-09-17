import QtQuick
import ".." as Kiki

// A yes/no question for destructive actions (plan 23): permanent delete, Empty Trash.
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 96
    property string title: ""
    property string message: ""
    property string confirmLabel: "Delete"
    property bool danger: true
    property var _cb: null
    function ask(opts, cb) { title = opts.title || ""; message = opts.message || ""; confirmLabel = opts.label || "Delete"; danger = opts.danger !== false; _cb = cb; visible = true; box.forceActiveFocus() }
    function answer(yes) { visible = false; const cb = _cb; _cb = null; if (cb) cb(yes) }
    MouseArea { anchors.fill: parent; onClicked: dlg.answer(false) }
    Rectangle {
        id: box
        anchors.centerIn: parent; width: 460; height: col.height + 44; color: Kiki.Theme.bg; border.width: 2; border.color: dlg.danger ? Kiki.Theme.red : Kiki.Theme.accent
        focus: true
        Keys.onEscapePressed: dlg.answer(false)
        Keys.onReturnPressed: dlg.answer(true)
        Keys.onEnterPressed: dlg.answer(true)
        MouseArea { anchors.fill: parent }
        Column {
            id: col; x: 22; y: 22; width: parent.width - 44; spacing: 12
            Text { text: dlg.title; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            Text { width: parent.width; wrapMode: Text.WordWrap; text: dlg.message; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Row {
                spacing: 8; anchors.right: parent.right
                Button { text: "Cancel"; onClicked: dlg.answer(false) }
                Rectangle {
                    width: okText.width + 32; height: 30; radius: 2; color: dlg.danger ? Kiki.Theme.red : Kiki.Theme.accent
                    Text { id: okText; anchors.centerIn: parent; text: dlg.confirmLabel + "  ⏎"; color: Kiki.Theme.bg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                    MouseArea { anchors.fill: parent; onClicked: dlg.answer(true) }
                }
            }
        }
    }
}
