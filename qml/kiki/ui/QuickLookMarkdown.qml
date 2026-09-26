import QtQuick
import ".." as Kiki
import "markdown_toc.js" as Toc

// A Markdown document in Quick Look (docs/0.2.0/05-quicklook.md, decision 3): the file rendered
// by Qt's own Markdown support, read-only, with its headings down the left as a table of
// contents that folds to a strip. The window loads this over its content area and hands it the
// file; the daemon's `ReadText` gives the text, capped, and says when it was cut.
Item {
    id: view
    /// A local `file://` URI: the window fetches a remote file first and shows the copy.
    property string uri: ""
    property string name: ""
    property var row: null
    /// Resolved lazily so a test can hand in a fake; the real singleton owns a socket.
    property var daemon: null
    function d() { return daemon || Kiki.Daemon }
    focus: true

    property string text: ""
    property int bytes: 0
    property bool truncated: false
    /// The daemon's sentence when the file could not be read, already in the window's language.
    property string error: ""
    /// `[{ level, text, line }]` from the source (`markdown_toc.js`).
    property var headings: []
    readonly property bool hasToc: headings.length > 0
    /// The fold is remembered across files and windows, so it is the setting itself: toggling
    /// writes the setting and the binding follows.
    readonly property bool folded: Kiki.Settings.view.quickLookToc === true
    /// The heading at or above the top of the view, -1 before the first.
    readonly property int current: {
        let at = -1
        const top = docFlick.contentY + view.margin + 1
        for (let i = 0; i < _ys.length; i++) if (_ys[i] >= 0 && _ys[i] <= top) at = i
        return at
    }
    /// Where each heading sits in the rendered document, -1 when its text was not found there.
    property var _ys: []
    /// Around the document: the same on every side, and what a jump leaves above a heading.
    readonly property int margin: 24
    /// Reading size — the UI font, two steps up from the lists. Code blocks keep the fixed font
    /// Qt gives them.
    readonly property int readingSize: 15
    /// About 78 characters of the reading font: wider than that a line is work to follow back.
    readonly property int maxLine: Math.ceil(readingFont.averageCharacterWidth * 78)
    FontMetrics { id: readingFont; font.family: Kiki.Theme.mono; font.pixelSize: view.readingSize }
    FontMetrics { id: tocFont; font.family: Kiki.Theme.mono; font.pixelSize: 12 }

    /// The column is as wide as its longest heading, indent and all, or its title — measured, so
    /// no language's words are clipped by design — and never more than a third of the window.
    readonly property int tocWidth: {
        let w = Math.ceil(tocFont.advanceWidth(Kiki.T.tr("quicklook.md.contents"))) + 12 + fold.width + 8
        for (const h of headings) w = Math.max(w, 12 + (h.level - 1) * 12 + Math.ceil(tocFont.advanceWidth(Toc.plain(h.text))) + 16)
        return Kiki.T.language ? Math.min(w, Math.floor(view.width / 3)) : w        // re-measured when the language changes
    }

    onUriChanged: load()
    function load() {
        text = ""; bytes = 0; truncated = false; error = ""; headings = []; _ys = []
        docFlick.contentY = 0
        if (!uri) return
        const u = uri
        d().request("ReadText", { uri: u }, (ok, err) => {
            // An answer for a file already left — or for a view already gone (the window closed
            // or switched kind while the daemon was reading), which the text view was seen to do.
            if (!view || u !== view.uri) return
            if (!ok) { view.error = err && err.message ? err.message : ""; return }
            view.headings = Toc.headings(ok.text || "")
            view.bytes = ok.bytes || 0
            view.truncated = ok.truncated === true
            view.text = ok.text || ""
        })
    }

    /// Find each heading in the rendered document, in order, so a repeated heading finds its own
    /// occurrence: a match that fills a whole block is the heading; failing one, any match.
    function locate() {
        if (!text || !headings.length) { _ys = []; return }
        const all = doc.getText(0, doc.length)
        const sep = " "
        let from = 0
        const ys = []
        for (const h of headings) {
            const needle = Toc.plain(h.text)
            let at = -1
            if (needle) {
                let p = all.indexOf(needle, from)
                while (p >= 0) {
                    const end = p + needle.length
                    if ((p === 0 || all[p - 1] === sep) && (end === all.length || all[end] === sep)) { at = p; break }
                    p = all.indexOf(needle, p + 1)
                }
                if (at < 0) at = all.indexOf(needle, from)
            }
            if (at < 0) { ys.push(-1); continue }
            from = at + needle.length
            ys.push(doc.y + doc.positionToRectangle(at).y)
        }
        _ys = ys
    }
    function jump(i) {
        if (i < 0 || i >= _ys.length || _ys[i] < 0) return
        docFlick.contentY = Math.max(0, Math.min(_ys[i] - margin, docFlick.contentHeight - docFlick.height))
    }
    function toggleFold() { if (hasToc) Kiki.Settings.set("view", "quickLookToc", !folded) }
    function scrollBy(dy) { docFlick.contentY = Math.max(0, Math.min(docFlick.contentY + dy, Math.max(0, docFlick.contentHeight - docFlick.height))) }

    // The document's keys; the window keeps the rest (Esc, Space, stepping to the next file).
    Keys.onPressed: event => {
        switch (event.key) {
        case Qt.Key_PageDown: scrollBy(docFlick.height - 2 * margin); break
        case Qt.Key_PageUp: scrollBy(-(docFlick.height - 2 * margin)); break
        case Qt.Key_Home: docFlick.contentY = 0; break
        case Qt.Key_End: scrollBy(docFlick.contentHeight); break
        case Qt.Key_T: if (event.modifiers !== Qt.NoModifier) return; toggleFold(); break
        // Selecting is the reader's, so copying must be too: the document never takes the focus.
        case Qt.Key_C: if (event.modifiers !== Qt.ControlModifier) return; doc.copy(); break
        default: return
        }
        event.accepted = true
    }

    // ---------------------------------------------------------------- the contents column
    Rectangle {
        id: tocCol
        objectName: "ql-md-toc"
        visible: view.hasToc
        anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom
        width: view.folded ? fold.width + 8 : view.tocWidth
        color: Kiki.Theme.bgDark
        Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Kiki.Theme.line }
        Item {
            id: tocHead
            width: parent.width; height: 36
            // The chevron points the way the column will go: left to fold away, right to come back.
            Rectangle {
                id: fold
                objectName: "ql-md-toc-fold"
                width: 28; height: 28; radius: 2
                anchors.left: parent.left; anchors.leftMargin: 4; anchors.verticalCenter: parent.verticalCenter
                color: foldHover.containsMouse ? Kiki.Theme.surface : "transparent"
                Icon { anchors.centerIn: parent; name: "chev-r"; size: 14; rotation: view.folded ? 0 : 180; color: foldHover.containsMouse ? Kiki.Theme.accent : Kiki.Theme.chrome }
                Tip { visible: foldHover.containsMouse; text: view.folded ? Kiki.T.tr("quicklook.md.unfold") : Kiki.T.tr("quicklook.md.fold") }
                MouseArea { id: foldHover; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: view.toggleFold() }
            }
            Text {
                objectName: "ql-md-toc-title"
                visible: !view.folded
                anchors.left: fold.right; anchors.leftMargin: 8; anchors.right: parent.right; anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                text: Kiki.T.tr("quicklook.md.contents").toUpperCase(); color: Kiki.Theme.muted
                elide: Text.ElideRight
                font.family: Kiki.Theme.mono; font.pixelSize: 10; font.bold: true; font.letterSpacing: 1
            }
        }
        Flickable {
            id: tocList
            visible: !view.folded
            anchors.top: tocHead.bottom; anchors.bottom: parent.bottom; anchors.bottomMargin: 8
            width: parent.width
            contentWidth: width; contentHeight: tocRows.height
            clip: true; boundsBehavior: Flickable.StopAtBounds
            NaturalScroll { }
            Column {
                id: tocRows
                width: parent.width
                Repeater {
                    model: view.headings
                    delegate: Rectangle {
                        id: tocRow
                        required property var modelData
                        required property int index
                        objectName: "ql-md-toc-row-" + index
                        readonly property bool current: view.current === index
                        width: tocRows.width; height: 24
                        color: rowHover.containsMouse ? Qt.rgba(1, 1, 1, 0.03) : "transparent"
                        Rectangle { visible: tocRow.current; width: 2; height: parent.height; color: Kiki.Theme.accent }
                        Text {
                            objectName: "label"
                            x: 12 + (tocRow.modelData.level - 1) * 12
                            width: parent.width - x - 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: Toc.plain(tocRow.modelData.text)
                            elide: Text.ElideRight
                            color: tocRow.current ? Kiki.Theme.accent : (tocRow.modelData.level === 1 ? Kiki.Theme.fg : Kiki.Theme.fgDim)
                            font.family: Kiki.Theme.mono; font.pixelSize: 12
                        }
                        MouseArea { id: rowHover; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: view.jump(tocRow.index) }
                    }
                }
            }
            // The marked row stays in sight as the document scrolls under it.
            Connections {
                target: view
                function onCurrentChanged() {
                    const y = view.current * 24
                    if (y < tocList.contentY) tocList.contentY = y
                    else if (y + 24 > tocList.contentY + tocList.height) tocList.contentY = Math.max(0, y + 24 - tocList.height)
                }
            }
        }
    }

    // ---------------------------------------------------------------- the document
    Flickable {
        id: docFlick
        objectName: "ql-md-flick"
        anchors.left: tocCol.visible ? tocCol.right : parent.left
        anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom
        contentWidth: width; contentHeight: docBody.height
        clip: true; boundsBehavior: Flickable.StopAtBounds
        NaturalScroll { }
        Item {
            id: docBody
            width: docFlick.width
            height: view.margin + doc.height + (cut.visible ? cut.height + view.margin : 0) + view.margin
            TextEdit {
                id: doc
                objectName: "ql-md-doc"
                // Centred, no wider than a line worth reading.
                width: Math.max(80, Math.min(view.maxLine, parent.width - 2 * view.margin))
                x: Math.round((parent.width - width) / 2); y: view.margin
                readOnly: true
                selectByMouse: true
                // Keys stay with the view: a click to select must not hand Home/End to the cursor.
                activeFocusOnPress: false
                textFormat: TextEdit.MarkdownText
                text: view.text
                // A picture beside the file resolves against the file's folder.
                baseUrl: view.uri ? view.uri.slice(0, view.uri.lastIndexOf("/") + 1) : ""
                wrapMode: TextEdit.WordWrap
                color: Kiki.Theme.fg
                selectionColor: Kiki.Theme.accent; selectedTextColor: Kiki.Theme.bg
                font.family: Kiki.Theme.mono; font.pixelSize: view.readingSize
                onLinkActivated: link => Qt.openUrlExternally(link)
                // Where the headings sit changes with the text and with every reflow.
                onTextChanged: Qt.callLater(view.locate)
                onWidthChanged: Qt.callLater(view.locate)
                onContentHeightChanged: Qt.callLater(view.locate)
            }
            // The daemon stopped reading at its cap: say so where the reader reaches it.
            Text {
                id: cut
                objectName: "ql-md-cut"
                visible: view.truncated
                anchors.top: doc.bottom; anchors.topMargin: view.margin; anchors.horizontalCenter: doc.horizontalCenter
                text: Kiki.T.tr("quicklook.md.cut", { size: Kiki.Format.bytes(view.bytes) })
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
        }
    }
    Text {
        objectName: "ql-md-error"
        visible: view.error !== ""
        anchors.centerIn: docFlick; width: Math.min(docFlick.width - 2 * view.margin, 480)
        horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
        text: view.error; color: Kiki.Theme.muted
        font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
    }
}
