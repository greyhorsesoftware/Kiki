import QtQuick
import ".." as Kiki

// AI query (plan 19): a chat about the selected files, beside the listing.
Rectangle {
    id: ai
    property var uris: []
    property string home: ""
    property var transcript: []      // [{ role, text, local }]
    property bool busy: false
    property int activeId: 0
    property var usage: ({ input: 0, output: 0 })
    property var sessions: ({})      // key -> transcript, remembered per file set
    signal close()
    color: Kiki.Theme.bg
    Rectangle { width: 1; height: parent.height; color: Kiki.Theme.line }

    function key() { return uris.join("\n") }
    function openFor(u) {
        if (uris.length) sessions[key()] = transcript
        uris = u; transcript = sessions[key()] || []; busy = false
        input.forceActiveFocus()
    }
    function ask(text) {
        if (!text.trim() || busy) return
        transcript = transcript.concat([{ role: "user", text: text }, { role: "assistant", text: "", local: false }])
        busy = true; input.text = ""
        const history = transcript.slice(0, -2).map(t => ({ role: t.role, text: t.text }))
        Kiki.Daemon.request("AiQuery", { session: key(), uris: uris, question: text, history: history }, ok => { if (ok) activeId = ok.id })
    }
    function update(text, local) { const t = transcript.slice(); t[t.length - 1] = { role: "assistant", text: text, local: local }; transcript = t }
    Connections {
        target: Kiki.Daemon
        function onEvent(msg) {
            if (msg.id !== ai.activeId) return
            if (msg.event === "AiDelta") ai.update(ai.transcript[ai.transcript.length - 1].text + msg.text, false)
            else if (msg.event === "AiDone") { ai.update(msg.text, msg.local); ai.busy = false; if (msg.usage) ai.usage = { input: ai.usage.input + (msg.usage.input || 0), output: ai.usage.output + (msg.usage.output || 0) } }
            else if (msg.event === "AiError") { ai.update("Error: " + msg.message, false); ai.busy = false }
        }
    }

    Column {
        anchors.fill: parent; anchors.margins: 12; spacing: 10
        Row {
            width: parent.width; height: 28; spacing: 8
            Icon { name: "info"; color: Kiki.Theme.accent; anchors.verticalCenter: parent.verticalCenter }
            Text { width: parent.width - 60; elide: Text.ElideMiddle; anchors.verticalCenter: parent.verticalCenter; text: ai.uris.length === 1 ? decodeURIComponent(ai.uris[0].split("/").pop()) : ai.uris.length + " files"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
            Icon { name: "x"; size: 12; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter; MouseArea { anchors.fill: parent; anchors.margins: -6; onClicked: ai.close() } }
        }
        ListView {
            id: log
            width: parent.width; height: parent.height - 28 - 10 - 44 - 10 - 20; clip: true; spacing: 10
            model: ai.transcript
            onCountChanged: positionViewAtEnd()
            delegate: Item {
                required property var modelData
                width: log.width; height: bubble.height
                Rectangle {
                    id: bubble
                    width: Math.min(log.width - 20, t.implicitWidth + 24); height: t.implicitHeight + 16; radius: 2
                    anchors.right: modelData.role === "user" ? parent.right : undefined
                    color: modelData.role === "user" ? Kiki.Theme.surface : "transparent"
                    Text { id: t; x: 12; y: 8; width: Math.min(log.width - 44, implicitWidth); wrapMode: Text.Wrap; text: modelData.text || (ai.busy ? "…" : ""); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12; lineHeight: 1.35 }
                    Text { visible: modelData.local; anchors.top: bubble.bottom; text: "computed locally"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 10 }
                }
            }
        }
        Rectangle {
            width: parent.width; height: 44; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: input.activeFocus ? Kiki.Theme.accent : Kiki.Theme.gutter
            TextEdit {
                id: input
                anchors.fill: parent; anchors.margins: 8; wrapMode: TextEdit.Wrap
                color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12; selectionColor: Kiki.Theme.accent
                Keys.onPressed: event => {
                    if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !(event.modifiers & Qt.ShiftModifier)) { ai.ask(text); event.accepted = true }
                    else if (event.key === Qt.Key_Escape) { ai.close(); event.accepted = true }
                }
                Text { visible: !input.text.length && !input.activeFocus; text: "Ask about this file… (count lines, how many times does \"x\" appear)"; color: Kiki.Theme.muted; font: input.font; width: parent.width; wrapMode: Text.Wrap }
            }
        }
        Text { text: (ai.usage.input + ai.usage.output) ? ai.usage.input + " in · " + ai.usage.output + " out tokens this session" : "Enter to send · Shift+Enter for a newline"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 10 }
    }
}
