import QtQuick
import ".." as Kiki

// The inline rename editor (F2): a text box over a row's name, in the row itself. One component,
// because list view and columns view put the same thing in the same place — the name, with the
// stem preselected, Enter to rename and Escape to leave it alone — and two copies would drift.
// It knows nothing about listings or panes: it is handed a name and answers with one.
Rectangle {
    id: box
    /// The name to start from.
    property string name: ""
    /// Enter, with a name that is not the one it started with.
    signal renamed(string name)
    /// It is over: accepted, escaped, or the focus went elsewhere. Always emitted before
    /// `renamed`, so whoever opened it can close it and then act.
    signal dismissed()
    radius: 2; z: 2
    color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.accent
    // The stem, not the extension: renaming a file almost never means renaming its kind.
    onVisibleChanged: if (visible) { edit.text = box.name; edit.forceActiveFocus(); const dot = edit.text.lastIndexOf("."); edit.select(0, dot > 0 ? dot : edit.text.length) }
    TextInput {
        id: edit; objectName: "renameEditor"; anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6; verticalAlignment: TextInput.AlignVCenter
        color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
        onAccepted: { const n = text; box.dismissed(); if (n && n !== box.name) box.renamed(n) }
        Keys.onEscapePressed: box.dismissed()
        onActiveFocusChanged: if (!activeFocus && box.visible) box.dismissed()
    }
}
