import QtQuick
import ".." as Kiki

// The inspector's Code tab: highlighted lines from the daemon's text view, gutter, current line, find.
Item {
    id: code
    property string uri: ""
    property string home: ""
    property int current: 0
    property string lang: ""
    property string findText: ""
    property var matches: []
    property int matchIndex: -1
    property bool wrap: true
    signal edit(string uri, int line)

    property Kiki.WindowCache lines: Kiki.WindowCache { padAhead: 200; padBehind: 100 }

    onUriChanged: reload()
    function reload() {
        lines.close(); current = 0; matches = []; matchIndex = -1
        if (!uri) return
        lines.lid = Kiki.Daemon.allocLid(); Kiki.Daemon.bind(lines.lid, lines)
        lines._rows = ({}); lines.count = 0
        Kiki.Daemon.request("OpenText", { lid: lines.lid, uri: uri }, (ok, err) => {
            if (err) { lang = "unsupported"; return }
            lang = ok.lang; lines.count = ok.total; lines.done = true
            lines._request(0, 200)
        })
    }
    function classColor(c) {
        switch (c) {
        case "keyword": return Kiki.Theme.purple
        case "string": return Kiki.Theme.green
        case "comment": return Kiki.Theme.muted
        case "number": case "constant": return Kiki.Theme.yellow
        case "type": return Kiki.Theme.yellow
        case "function": return Kiki.Theme.accent
        case "tag": return Kiki.Theme.red
        case "attribute": case "property": return Kiki.Theme.cyan
        case "operator": case "punctuation": return Kiki.Theme.fgDim
        default: return Kiki.Theme.fg
        }
    }
    function render(spans) {
        if (!spans) return ""
        const esc = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/ /g, "&nbsp;").replace(/\t/g, "&nbsp;".repeat(Kiki.Settings.editor.tabWidth || 4))
        return spans.map(s => "<span style='color:" + classColor(s.class) + "'>" + esc(s.text) + "</span>").join("")
    }
    function go(n) { current = Math.max(0, Math.min(lines.count - 1, n)); view.positionViewAtIndex(current, ListView.Contain) }
    function find(text) {
        findText = text; matches = []; matchIndex = -1
        if (!text) return
        // Search what the cache holds around the viewport first; a full scan goes through the daemon in a later version.
        const q = text.toLowerCase()
        for (const k in lines._rows) { const r = lines._rows[k]; if (r.spans.map(s => s.text).join("").toLowerCase().includes(q)) matches.push(Number(k)) }
        matches.sort((a, b) => a - b)
        if (matches.length) { matchIndex = 0; go(matches[0]) }
    }
    function nextMatch(d) { if (!matches.length) return; matchIndex = (matchIndex + d + matches.length) % matches.length; go(matches[matchIndex]) }

    Column {
        anchors.fill: parent
        Row {
            width: parent.width; height: 28; spacing: 8
            Text { anchors.verticalCenter: parent.verticalCenter; text: code.lang; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { anchors.verticalCenter: parent.verticalCenter; text: (code.current + 1) + ":" + code.lines.count; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Item { width: parent.width - 260; height: 1 }
            Rectangle {
                width: 150; height: 24; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: findInput.activeFocus ? Kiki.Theme.accent : Kiki.Theme.line
                TextInput { id: findInput; anchors.fill: parent; anchors.margins: 6; verticalAlignment: TextInput.AlignVCenter; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 11; selectionColor: Kiki.Theme.accent; onAccepted: code.find(text)
                    Text { visible: !parent.text.length && !parent.activeFocus; text: "find  /"; color: Kiki.Theme.muted; font: parent.font; anchors.verticalCenter: parent.verticalCenter } }
            }
            Text { anchors.verticalCenter: parent.verticalCenter; visible: code.matches.length > 0; text: (code.matchIndex + 1) + "/" + code.matches.length; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
        }
        ListView {
            id: view
            width: parent.width; height: parent.height - 28
            clip: true; reuseItems: true; model: code.lines.count
            onContentYChanged: code.lines.setViewport(Math.max(0, Math.floor(contentY / 20)), Math.ceil(height / 20) + 1)
            Connections { target: code.lines; function onReset() { view.forceLayout() } }
            delegate: Rectangle {
                id: ln
                required property int index
                property var r: code.lines.row(index)
                width: view.width; height: Math.max(20, txt.implicitHeight)
                color: index === code.current ? Kiki.Theme.surface : "transparent"
                Connections { target: code.lines; function onRowsUpdated(first, n) { if (ln.index >= first && ln.index < first + n) ln.r = code.lines.row(ln.index) } }
                Row {
                    anchors.fill: parent
                    Text { width: 44; height: 20; horizontalAlignment: Text.AlignRight; rightPadding: 12; text: ln.index + 1; color: ln.index === code.current ? Kiki.Theme.fg : Kiki.Theme.gutter; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                    Rectangle { width: 1; height: parent.height; color: Kiki.Theme.line }
                    Text { id: txt; x: 8; width: parent.width - 60; leftPadding: 8; textFormat: Text.RichText; wrapMode: code.wrap ? Text.WrapAnywhere : Text.NoWrap; text: ln.r ? code.render(ln.r.spans) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12; lineHeight: 1.25 }
                }
                MouseArea { anchors.fill: parent; onClicked: code.current = ln.index; onDoubleClicked: code.edit(code.uri, ln.index + 1) }
            }
        }
    }
    Keys.onPressed: event => {
        switch (event.key) {
        case Qt.Key_J: case Qt.Key_Down: code.go(code.current + 1); break
        case Qt.Key_K: case Qt.Key_Up: code.go(code.current - 1); break
        case Qt.Key_G: code.go(event.modifiers & Qt.ShiftModifier ? code.lines.count - 1 : 0); break
        case Qt.Key_Slash: findInput.forceActiveFocus(); break
        case Qt.Key_N: code.nextMatch(event.modifiers & Qt.ShiftModifier ? -1 : 1); break
        case Qt.Key_E: code.edit(code.uri, code.current + 1); break
        default: return
        }
        event.accepted = true
    }
}
