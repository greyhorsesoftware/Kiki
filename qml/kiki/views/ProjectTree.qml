import QtQuick
import ".." as Kiki
import "../ui" as UI

// Project mode (plan 16): the narrow tree of one project root.
Item {
    id: root
    property string rootUri: ""
    property string home: ""
    property var repo: null
    property int current: -1
    signal openFile(string uri)
    signal sendToAgent(string uri)
    signal leave()

    property Kiki.WindowCache tree: Kiki.WindowCache { padAhead: 100; padBehind: 50 }

    onRootUriChanged: reload()
    function reload() {
        tree.close(); current = -1
        if (!rootUri) return
        tree.lid = Kiki.Daemon.allocLid(); Kiki.Daemon.bind(tree.lid, tree)
        tree._rows = ({}); tree.count = 0
        Kiki.Daemon.request("OpenTree", { lid: tree.lid, uri: rootUri }, (ok, err) => { if (ok) { tree.count = ok.n; tree.done = true; tree._request(0, 200) } })
    }
    function toggle(i) { const r = tree.row(i); if (!r || !r.isDir) return; Kiki.Daemon.request("TreeExpand", { lid: tree.lid, first: i, expanded: !r.expanded }) }
    function activate(i) { const r = tree.row(i); if (!r) return; if (r.isDir) toggle(i); else root.openFile(r.uri) }
    function filter(text) { Kiki.Daemon.request("TreeFilter", { lid: tree.lid, text: text }) }
    function reveal(uri) { Kiki.Daemon.request("TreeReveal", { lid: tree.lid, uri: uri }, ok => { if (ok) { current = ok.row; list.positionViewAtIndex(ok.row, ListView.Contain) } }) }
    function move(d) { current = Math.max(0, Math.min(tree.count - 1, current + d)); list.positionViewAtIndex(current, ListView.Contain) }

    Column {
        anchors.fill: parent
        Rectangle {
            width: parent.width; height: 44; color: Kiki.Theme.bg
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
            Row {
                anchors.fill: parent; anchors.leftMargin: 12; anchors.rightMargin: 12; spacing: 8
                UI.Icon { anchors.verticalCenter: parent.verticalCenter; name: "folder"; color: Kiki.Theme.accent }
                Text { anchors.verticalCenter: parent.verticalCenter; text: Kiki.Format.crumbs(root.rootUri, root.home).slice(-1)[0] || ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true; elide: Text.ElideRight; width: Math.min(implicitWidth, parent.width - 160) }
                Rectangle { visible: root.repo && root.repo.branch; anchors.verticalCenter: parent.verticalCenter; height: 20; width: br.width + 12; radius: 2; color: "transparent"; border.width: 1; border.color: Kiki.Theme.gutter
                    Row { id: br; anchors.centerIn: parent; spacing: 4
                        UI.Icon { name: "mirror"; size: 10; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
                        Text { text: root.repo ? root.repo.branch + (root.repo.ahead ? " ↑" + root.repo.ahead : "") : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 } } }
                Item { width: parent.width - 220; height: 1 }
                UI.Button { anchors.verticalCenter: parent.verticalCenter; text: "Leave"; height: 24; onClicked: root.leave() }
            }
        }
        Rectangle {
            width: parent.width - 20; x: 10; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: filterInput.activeFocus ? Kiki.Theme.accent : Kiki.Theme.line
            anchors.horizontalCenter: parent.horizontalCenter
            Row { anchors.fill: parent; anchors.leftMargin: 8; spacing: 8
                UI.Icon { name: "search"; size: 12; color: Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
                TextInput { id: filterInput; width: parent.width - 30; height: parent.height; verticalAlignment: TextInput.AlignVCenter; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12; selectionColor: Kiki.Theme.accent; onTextChanged: root.filter(text)
                    Text { visible: !parent.text.length && !parent.activeFocus; text: "Filter tree"; color: Kiki.Theme.muted; font: parent.font; anchors.verticalCenter: parent.verticalCenter } } }
        }
        Item { width: 1; height: 6 }
        ListView {
            id: list
            UI.NaturalScroll { }
            width: parent.width; height: parent.height - 44 - 30 - 6
            clip: true; reuseItems: true; model: root.tree.count
            onContentYChanged: root.tree.setViewport(Math.max(0, Math.floor(contentY / 26)), Math.ceil(height / 26) + 1)
            Connections { target: root.tree; function onReset() { list.forceLayout() } }
            delegate: Rectangle {
                id: tr
                required property int index
                property var r: root.tree.row(index)
                property bool sel: index === root.current
                width: list.width; height: 26
                color: sel ? Kiki.Theme.accent : "transparent"
                Connections { target: root.tree; function onRowsUpdated(first, n) { if (tr.index >= first && tr.index < first + n) tr.r = root.tree.row(tr.index) } function onReset() { tr.r = root.tree.row(tr.index) } }
                Row {
                    anchors.fill: parent; anchors.leftMargin: 12 + (tr.r ? tr.r.depth * 16 : 0); anchors.rightMargin: 10; spacing: 6
                    Item { width: 12; height: 12; anchors.verticalCenter: parent.verticalCenter
                        UI.Icon { visible: tr.r && tr.r.isDir; anchors.centerIn: parent; name: tr.r && tr.r.expanded ? "chev-d" : "chev-r"; size: 12; color: tr.sel ? Kiki.Theme.bg : Kiki.Theme.muted }
                        MouseArea { anchors.fill: parent; anchors.margins: -6; onClicked: root.toggle(tr.index) } }
                    UI.Icon { anchors.verticalCenter: parent.verticalCenter; name: tr.r ? tr.r.kind : "file"; size: 14; color: tr.sel ? Kiki.Theme.bg : Kiki.Theme.kindColor(tr.r ? tr.r.kind : "file") }
                    Text { anchors.verticalCenter: parent.verticalCenter; width: parent.width - 18 - 20 - 20; elide: Text.ElideRight; text: tr.r ? tr.r.name : ""; color: tr.sel ? Kiki.Theme.bg : (tr.r && tr.r.git && tr.r.git.state === "ignored" ? Kiki.Theme.muted : Kiki.Theme.fg); font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    Text { anchors.verticalCenter: parent.verticalCenter; width: 14; horizontalAlignment: Text.AlignHCenter; text: Kiki.Format.gitBadge(tr.r ? tr.r.git : null); color: tr.sel ? Kiki.Theme.bg : Kiki.Format.gitColor(tr.r ? tr.r.git : null); font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true }
                }
                MouseArea { anchors.fill: parent; anchors.leftMargin: 30; onClicked: { root.current = tr.index; root.activate(tr.index) } }
            }
        }
    }
    Keys.onPressed: event => {
        const r = root.tree.row(root.current)
        switch (event.key) {
        case Qt.Key_J: case Qt.Key_Down: root.move(1); break
        case Qt.Key_K: case Qt.Key_Up: root.move(-1); break
        case Qt.Key_Space: root.toggle(root.current); break
        case Qt.Key_H: if (r && r.isDir && r.expanded) root.toggle(root.current); break
        case Qt.Key_L: if (r && r.isDir && !r.expanded) root.toggle(root.current); break
        case Qt.Key_Return: case Qt.Key_Enter: if (event.modifiers & Qt.AltModifier) { if (r) root.sendToAgent(r.uri) } else root.activate(root.current); break
        case Qt.Key_Slash: filterInput.forceActiveFocus(); break
        case Qt.Key_Escape: root.leave(); break
        default: return
        }
        event.accepted = true
    }
}
