import QtQuick
import ".." as Kiki

// One field of a plugin-described form: text, password, path, port, select, file, browse — and
// `keys`, a list of what the plugin found with a tick beside each.
Column {
    id: f
    property var field: ({})            // { key, label, kind, required, default, options }
    objectName: "field-" + (field.key || "")
    property string value: ""
    property string error: ""
    signal edited(string value)
    signal browse()
    /// A `path` field that names a folder on THIS machine: its folder icon opens a chooser. Off
    /// for a remote path, which a local chooser could not answer.
    property bool pickable: false
    signal pick()
    /// For `keys`: what the plugin found, `[{ value, label }]`, handed in by the dialog. The
    /// field's value is the ticked ones, one per line, and only those are offered to the server;
    /// none ticked is a password-only location. The dialog ticks the first one to begin with.
    property var choices: []
    readonly property var picked: value ? value.split("\n").filter(v => v !== "") : []
    function toggle(v) {
        const p = picked.slice(); const i = p.indexOf(v)
        if (i >= 0) p.splice(i, 1); else p.push(v)
        // Kept in the order they are listed, which is the order they will be offered in.
        const order = choices.map(c => c.value)
        p.sort((a, b) => (order.indexOf(a) < 0 ? 1e9 : order.indexOf(a)) - (order.indexOf(b) < 0 ? 1e9 : order.indexOf(b)))
        // Announced, not assigned: `value` is bound to the form's own record, and assigning it
        // here would cut that binding — the ticks would then survive the form being reset or
        // opened for another location.
        edited(p.join("\n"))
    }
    width: parent ? parent.width : 300
    spacing: 6
    Text { text: field.label ? field.label.toUpperCase() : ""; color: f.error ? Kiki.Theme.danger : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.letterSpacing: 0.6 }
    Rectangle {
        objectName: "keys-box"
        visible: f.field.kind === "keys"
        width: parent.width; height: keyRows.height + 12; radius: 2
        color: Kiki.Theme.bgDark; border.width: 1; border.color: f.error ? Kiki.Theme.danger : Kiki.Theme.gutter
        Column {
            id: keyRows
            x: 6; y: 6; width: parent.width - 12
            // Nothing found is said, not left as an empty box.
            Text {
                objectName: "keys-none"
                visible: f.choices.length === 0 && f.picked.length === 0
                x: 6; height: 28; verticalAlignment: Text.AlignVCenter
                text: "No keys found in ~/.ssh"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
            }
            Repeater {
                model: f.choices
                delegate: Rectangle {
                    id: keyRow
                    required property var modelData
                    required property int index
                    objectName: "key-" + index
                    readonly property bool ticked: f.picked.indexOf(modelData.value) >= 0
                    width: keyRows.width; height: 28; radius: 2
                    color: keyHover.containsMouse ? Kiki.Theme.surface : "transparent"
                    Row {
                        anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6; spacing: 8
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 14; height: 14; radius: 2; color: keyRow.ticked ? Kiki.Theme.accent : "transparent"; border.width: 1; border.color: keyRow.ticked ? Kiki.Theme.accent : Kiki.Theme.gutter
                            Icon { visible: keyRow.ticked; anchors.centerIn: parent; name: "check"; size: 10; color: Kiki.Theme.bg } }
                        Icon { anchors.verticalCenter: parent.verticalCenter; name: "key"; size: 13; color: Kiki.Theme.muted }
                        Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 60; elide: Text.ElideMiddle; text: modelData.label || modelData.value; color: keyRow.ticked ? Kiki.Theme.fg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    }
                    MouseArea { id: keyHover; anchors.fill: parent; hoverEnabled: true; onClicked: f.toggle(modelData.value) }
                }
            }
            // A key named by a saved location that is no longer in ~/.ssh: still listed, so it
            // can be seen and unticked, rather than silently dropped.
            Repeater {
                model: f.picked.filter(v => !f.choices.some(c => c.value === v))
                delegate: Rectangle {
                    required property string modelData
                    width: keyRows.width; height: 28; radius: 2; color: "transparent"
                    Row {
                        anchors.fill: parent; anchors.leftMargin: 6; spacing: 8
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 14; height: 14; radius: 2; color: Kiki.Theme.accent }
                        Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 40; elide: Text.ElideMiddle; text: modelData + "  (not found)"; color: Kiki.Theme.yellow; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    }
                    MouseArea { anchors.fill: parent; onClicked: f.toggle(modelData) }
                }
            }
        }
    }
    Rectangle {
        visible: f.field.kind !== "keys"
        width: parent.width; height: 32; radius: 2
        color: Kiki.Theme.bgDark; border.width: 1; border.color: f.error ? Kiki.Theme.danger : (input.activeFocus ? Kiki.Theme.accent : Kiki.Theme.gutter)
        Row {
            anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 8; spacing: 8
            Icon {
                id: kindIcon
                objectName: "field-icon"
                visible: f.field.kind === "file" || f.field.kind === "path"; name: f.field.kind === "file" ? "key" : "folder"; size: 14
                color: f.pickable && pickArea.containsMouse ? Kiki.Theme.accent : Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter
                // A bigger target than a 14 px glyph, and a tip that says what it does.
                MouseArea { id: pickArea; objectName: "field-pick"; enabled: f.pickable; visible: f.pickable; anchors.centerIn: parent; width: 26; height: 26; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: f.pick() }
                Tip { visible: f.pickable && pickArea.containsMouse; text: "Choose a folder…" }
            }
            TextInput {
                activeFocusOnTab: true
                id: input
                visible: f.field.kind !== "select"
                width: parent.width - 30 - (f.field.kind === "browse" ? 80 : 0); height: parent.height; verticalAlignment: TextInput.AlignVCenter
                text: f.value
                echoMode: f.field.kind === "password" ? TextInput.Password : TextInput.Normal
                inputMethodHints: f.field.kind === "port" ? Qt.ImhDigitsOnly : Qt.ImhNone
                color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
                onTextChanged: if (text !== f.value) { f.value = text; f.edited(text) }
            }
            Text {
                visible: f.field.kind === "select"
                anchors.verticalCenter: parent.verticalCenter; width: parent.width - 30; elide: Text.ElideRight
                text: f.value; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
            }
            Icon { visible: f.field.kind === "select"; name: "chev-d"; size: 12; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
            // browse: the plugin lists choices (hosts, shares) for this field
            Rectangle { visible: f.field.kind === "browse"; anchors.verticalCenter: parent.verticalCenter; width: 76; height: 24; radius: 2; color: bh.containsMouse ? Kiki.Theme.surface : "transparent"; border.width: 1; border.color: Kiki.Theme.gutter
                Text { anchors.centerIn: parent; text: "Browse…"; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                MouseArea { id: bh; anchors.fill: parent; hoverEnabled: true; onClicked: f.browse() } }
        }
        MouseArea {
            visible: f.field.kind === "select"; anchors.fill: parent
            onClicked: { const o = f.field.options || []; const i = o.indexOf(f.value); f.value = o[(i + 1) % o.length]; f.edited(f.value) }
        }
    }
    Text { visible: f.error !== ""; text: f.error; color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
}
