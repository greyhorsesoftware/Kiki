import QtQuick
import ".." as Kiki

// A PDF in the Quick Look window (docs/0.2.0/05-quicklook.md, decision 3): its pages one under
// the other, drawn by the daemon — `PdfInfo` says how many and how page 1 is shaped, `PdfPage`
// renders one at a width into the thumbnail cache and answers with the PNG's path. Not
// `QtQuick.Pdf`, which on Arch comes with qt6-webengine: too much to ask of every install for a
// preview. What that gives up — selecting text, find — a look does not need.
//
// Zoom is the page width: `+`/`-` change it, `0` fits it to the window. A page is asked for as
// it scrolls into view and the answer kept, so scrolling back costs nothing; a resize or a zoom
// asks again at the new width once the size has held for a moment, the old picture scaled up
// in the meantime rather than a blank.
Item {
    id: pdf
    /// The file, a local `file://` uri: the daemon reads it from disk.
    property string uri: ""
    property string name: ""
    property var row: null
    /// Resolved lazily so a test can hand in a fake; the real singleton owns a socket.
    property var daemon: null
    function d() { return daemon || Kiki.Daemon }
    focus: true

    /// What `PdfInfo` said: the number of pages, and page 1's shape as height over width. The
    /// shape sizes every page until its own render says otherwise, so the list has its full
    /// length from the first answer and the page counter can follow the scroll at once.
    property int pages: 0
    property real pageAspect: 1.294
    /// The daemon's message when the document itself could not be opened; "" while it can.
    property string error: ""
    /// True from the first ask to the first answer, either way.
    readonly property bool loading: uri !== "" && pages === 0 && error === ""

    /// Air either side of a page, and the widest a fitted page gets: past 1400 px a page is a
    /// poster, and a render that wide for every page of a long document is memory for nothing.
    readonly property int margin: 24
    readonly property int maxFitWidth: 1400
    /// The page width as a factor of the fitted width; `0` on the keyboard puts it back to 1.
    property real zoom: 1
    readonly property int fitWidth: Math.max(64, Math.min(maxFitWidth, Math.round(width) - 2 * margin))
    /// The width a page is drawn at now — what the delegates size themselves by.
    readonly property int pageWidth: Math.max(64, Math.min(4 * maxFitWidth, Math.round(fitWidth * zoom)))
    /// The width the daemon was last asked to render at. It follows `pageWidth` after the
    /// debounce, not with it: a window being dragged wider changes its width every frame, and
    /// every one of those would otherwise be a render of every visible page.
    property int askedWidth: 0
    onPageWidthChanged: if (pages > 0) settle.restart()
    property Timer settle: Timer { interval: 250; onTriggered: pdf.askedWidth = pdf.pageWidth }

    /// `PdfPage` answers, by `page@width` — `{ path, width, height }`, or `{ error }` for a page
    /// the daemon could not draw. Kept on the view rather than in the delegates: the list lets
    /// a delegate go once it scrolls out of sight, and a page you have seen should not be
    /// rendered again on the way back up.
    property var rendered: ({})
    property var _inflight: ({})
    /// Which load an answer belongs to; a reply for a file the window has since left is dropped.
    property int _gen: 0

    // Not before the item is complete: a uri handed in with the item fires the change before
    // the anchors have given it a width, and the first render would be asked for at 64 px.
    property bool _ready: false
    onUriChanged: if (_ready) load()
    Component.onCompleted: { _ready = true; load() }
    function load() {
        settle.stop()
        _gen += 1
        pages = 0; error = ""; zoom = 1
        rendered = ({}); _inflight = ({})
        if (uri === "") return
        const gen = _gen
        d().request("PdfInfo", { uri: uri }, (ok, err) => {
            if (!pdf || gen !== pdf._gen) return          // a later file, or the view already gone
            if (err || !ok) { pdf.error = err && err.message ? err.message : Kiki.T.tr("error.failed"); return }
            if (ok.width > 0 && ok.height > 0) pdf.pageAspect = ok.height / ok.width
            // The width before the count: the delegates the count creates read it at once.
            pdf.askedWidth = pdf.pageWidth
            pdf.pages = ok.pages || 0
        })
    }
    /// Ask the daemon for `page` at the width in force, unless it is here or on its way.
    function ask(page) {
        if (uri === "" || askedWidth <= 0) return
        const w = askedWidth, key = page + "@" + w
        if (rendered[key] !== undefined || _inflight[key]) return
        _inflight[key] = true
        const gen = _gen
        d().request("PdfPage", { uri: uri, page: page, width: w }, (ok, err) => {
            if (!pdf || gen !== pdf._gen) return          // a later file, or the view already gone
            delete pdf._inflight[key]
            const m = Object.assign({}, pdf.rendered)
            m[key] = err || !ok || !ok.path ? { error: err && err.message ? err.message : Kiki.T.tr("error.failed") } : ok
            pdf.rendered = m
        })
    }

    // ---------------------------------------------------------------- zoom and scrolling
    /// The place on the page stays under the eye when the page changes size: the list's offset
    /// is scaled with the width, after the list has laid its delegates out at the new one.
    function setZoom(z) {
        const before = pageWidth
        zoom = Math.max(0.25, Math.min(4, z))
        if (pageWidth === before) return
        list.forceLayout()
        list.contentY = clampY(list.contentY * pageWidth / before)
    }
    function zoomIn() { setZoom(zoom * 1.25) }
    function zoomOut() { setZoom(zoom / 1.25) }
    function fit() { setZoom(1) }
    /// The list's own limits: its top is above its first page by the margin, not at 0.
    readonly property real topY: list.originY - list.topMargin
    readonly property real bottomY: Math.max(topY, list.originY + list.contentHeight + list.bottomMargin - list.height)
    function clampY(y) { return Math.max(topY, Math.min(y, bottomY)) }
    function scrollBy(dy) { list.contentY = clampY(list.contentY + dy) }

    /// The page under the eye, 1-based; 0 while there are none. A point two fifths of the way
    /// down the view rather than its top edge: the top edge is on the page above for as long
    /// as its tail is showing, and a counter that says "page 3" while page 4 fills the window
    /// reads as wrong.
    property int page: 0
    function follow() {
        if (pages === 0) { page = 0; return }
        const x = list.contentX + list.width / 2, y = list.contentY + list.height * 0.4
        let i = list.indexAt(x, y)
        // In the gap between two pages: the one below is where the eye is going.
        if (i < 0) i = list.indexAt(x, y + list.spacing)
        if (i >= 0) page = i + 1
    }

    Keys.onPressed: event => {
        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
        switch (event.key) {
        case Qt.Key_Plus: case Qt.Key_Equal: zoomIn(); break
        case Qt.Key_Minus: zoomOut(); break
        case Qt.Key_0: fit(); break
        case Qt.Key_PageDown: scrollBy(list.height * 0.9); break
        case Qt.Key_PageUp: scrollBy(-list.height * 0.9); break
        case Qt.Key_Home: list.contentY = topY; break
        case Qt.Key_End: list.contentY = bottomY; break
        case Qt.Key_Down: scrollBy(80); break
        case Qt.Key_Up: scrollBy(-80); break
        default:
            // `+` arrives as its own key on some layouts and as Shift+`=` with text "+" on others.
            if (event.text === "+") { zoomIn(); break }
            return                                   // the window's keys — step, close — pass on
        }
        event.accepted = true
    }

    // The paper is white whatever the theme; the desk under it is darker than the window, so the
    // pages are the brightest thing on screen and their edges read.
    Rectangle { anchors.fill: parent; color: Qt.darker(Kiki.Theme.bgDark, 1.25) }

    ListView {
        id: list
        objectName: "quicklook-pdf-pages"
        anchors.fill: parent
        model: pdf.pages
        spacing: 16
        topMargin: pdf.margin; bottomMargin: pdf.margin
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        // Only what is on screen: a page is rendered when it comes into view, not on the chance
        // it will. The daemon keeps every render, so a page seen once is quick the second time.
        cacheBuffer: 0
        // A page zoomed wider than the window scrolls sideways too.
        contentWidth: Math.max(width, pdf.pageWidth + 2 * pdf.margin)
        flickableDirection: Flickable.AutoFlickIfNeeded
        onContentYChanged: pdf.follow()
        onCountChanged: pdf.follow()
        onHeightChanged: pdf.follow()
        NaturalScroll { }

        delegate: Item {
            id: sheet
            required property int index
            objectName: "quicklook-pdf-sheet-" + (index + 1)
            readonly property int number: index + 1
            readonly property string key: number + "@" + pdf.askedWidth
            /// This page at the width in force: an answer, an error, or nothing yet. Read
            /// again when the width or the answers change, not bound: a binding on `rendered`
            /// re-entered itself when the daemon answered inside the ask (the fake does), and
            /// Qt called that a loop.
            property var got: null
            /// The last picture the daemon gave for this page at any width: what is drawn,
            /// scaled, while the one at the new width is on its way.
            property var shown: null
            function refresh() {
                got = pdf.rendered[key] === undefined ? null : pdf.rendered[key]
                if (got && got.path) shown = got
            }
            onKeyChanged: { refresh(); pdf.ask(number) }
            Component.onCompleted: { refresh(); pdf.ask(number) }
            Connections { target: pdf; function onRenderedChanged() { sheet.refresh() } }
            width: pdf.pageWidth
            height: Math.round(width * (shown && shown.width > 0 ? shown.height / shown.width : pdf.pageAspect))
            x: Math.max(pdf.margin, Math.round((list.contentWidth - width) / 2))

            Rectangle {
                anchors.fill: parent
                color: sheet.got && sheet.got.error ? Kiki.Theme.surface : "white"
                border.width: 1; border.color: Qt.rgba(0, 0, 0, 0.35)
            }
            Image {
                objectName: "quicklook-pdf-image"
                anchors.fill: parent
                visible: !(sheet.got && sheet.got.error)
                source: sheet.shown && sheet.shown.path ? "file://" + sheet.shown.path : ""
                asynchronous: true; smooth: true; mipmap: true; cache: false
                fillMode: Image.PreserveAspectFit
            }
            // The daemon has been asked and has not answered — the first look at this page.
            Text {
                visible: !sheet.shown && !(sheet.got && sheet.got.error)
                anchors.centerIn: parent
                text: Kiki.T.tr("quicklook.pdf.loading")
                color: Qt.rgba(0, 0, 0, 0.4); font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
            // The daemon could not draw this page: its sentence stands in the page's place, so the
            // document is still all there and one bad page does not hide the rest.
            Text {
                objectName: "quicklook-pdf-sheet-error"
                visible: !!(sheet.got && sheet.got.error)
                anchors.centerIn: parent
                width: Math.max(1, parent.width - 32)
                horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
                text: sheet.got && sheet.got.error ? sheet.got.error : ""
                color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
        }
    }

    // Where you are in the document, following the scroll; in a pill over the pages, so it is
    // readable on white paper and on the desk alike.
    Rectangle {
        visible: pdf.pages > 0
        anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 12
        width: pageText.implicitWidth + 20; height: pageText.implicitHeight + 10; radius: height / 2
        color: Qt.rgba(0, 0, 0, 0.55)
        border.width: 1; border.color: Qt.rgba(1, 1, 1, 0.25)
        Text {
            id: pageText
            objectName: "quicklook-pdf-page"
            anchors.centerIn: parent
            text: Kiki.T.tr("quicklook.pdf.page", { n: pdf.page, total: pdf.pages })
            color: "white"; font.family: Kiki.Theme.mono; font.pixelSize: 11
        }
    }

    // Before the daemon has said anything about the file, and when what it said was no.
    Text {
        objectName: "quicklook-pdf-loading"
        visible: pdf.loading
        anchors.centerIn: parent
        text: Kiki.T.tr("quicklook.pdf.loading")
        color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
    }
    Text {
        objectName: "quicklook-pdf-error"
        visible: pdf.error !== ""
        anchors.centerIn: parent
        width: Math.max(1, parent.width - 2 * pdf.margin)
        horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
        text: pdf.error
        color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
    }
}
