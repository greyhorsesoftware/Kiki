import QtQuick
import ".." as Kiki

// One field of a plugin-described form: text, password, path, port, select, file.
Column {
    id: f
    property var field: ({})            // { key, label, kind, required, default, options }
    property string value: ""
    property string error: ""
    signal edited(string value)
    width: parent ? parent.width : 300
    spacing: 6
    Text { text: field.label ? field.label.toUpperCase() : ""; color: f.error ? Kiki.Theme.red : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.letterSpacing: 0.6 }
    Rectangle {
        width: parent.width; height: 32; radius: 2
        color: Kiki.Theme.bgDark; border.width: 1; border.color: f.error ? Kiki.Theme.red : (input.activeFocus ? Kiki.Theme.accent : Kiki.Theme.gutter)
        Row {
            anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 8; spacing: 8
            Icon { visible: f.field.kind === "file" || f.field.kind === "path"; name: f.field.kind === "file" ? "key" : "folder"; size: 14; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
            TextInput {
                id: input
                visible: f.field.kind !== "select"
                width: parent.width - 30; height: parent.height; verticalAlignment: TextInput.AlignVCenter
                text: f.value
                echoMode: f.field.kind === "password" ? TextInput.Password : TextInput.Normal
                inputMethodHints: f.field.kind === "port" ? Qt.ImhDigitsOnly : Qt.ImhNone
                color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
                onTextChanged: if (text !== f.value) { f.value = text; f.edited(text) }
            }
            Text {
                visible: f.field.kind === "select"
                anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30
                text: f.value; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
            }
            Icon { visible: f.field.kind === "select"; name: "chev-d"; size: 12; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
        }
        MouseArea {
            visible: f.field.kind === "select"; anchors.fill: parent
            onClicked: { const o = f.field.options || []; const i = o.indexOf(f.value); f.value = o[(i + 1) % o.length]; f.edited(f.value) }
        }
    }
    Text { visible: f.error !== ""; text: f.error; color: Kiki.Theme.red; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
}
