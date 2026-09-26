import QtQuick
import Quickshell
import ".." as Kiki

// Quick Look (docs/0.2.0/05-quicklook.md): a file's own window, opened with Space on a file. A
// FloatingWindow of its own rather than a panel over the list — a preview is something you put
// beside the list and keep looking at while you move on, so the compositor moves and tiles it
// like any window. The file's name is the title, a close box top-right, Esc or Space closes.
// One window follows the selection: `show` replaces what is in it, and j k and the arrows
// inside it ask the shell to move the selection rather than opening a second one.
//
// What is shown is chosen by kind, each kind a component of its own loaded into `body`
// (QuickLookImage, QuickLookText, QuickLookMarkdown, QuickLookPdf, QuickLookVideo). A kind
// none of them takes — an archive, a spreadsheet, a binary — gets its icon, name, kind and
// size, and the way to an application; nothing is ever started from here on its own. A file on
// a server is fetched first (decision 4), its progress shown until the copy is there, and one
// over the size limit is offered rather than fetched. It is a look, not an editor: nothing is
// written.
FloatingWindow {
    id: ql
    /// A test's fake; the real singleton otherwise (as Theme resolves its daemon).
    property var daemon: null
    function d() { return daemon || Kiki.Daemon }
    property string home: ""
    /// The folder the pane is standing in. The header names where the file is only when it is
    /// somewhere else — a column further along, a search result, a server.
    property string paneUri: ""

    /// What is being shown: the listing row and its URI, as the pane gave them.
    property var row: null
    property string uri: ""
    readonly property string name: row && row.name ? row.name : (uri ? decodeURIComponent(uri.replace(/\/+$/, "").split("/").pop()) : "")
    readonly property string kind: row && row.kind ? row.kind : "file"
    readonly property real size: row ? (row.meta && row.meta.size !== undefined ? row.meta.size : (row.size || 0)) : 0
    readonly property bool isLocal: uri.indexOf("file://") === 0
    readonly property string folder: uri.replace(/\/[^/]*$/, "")
    readonly property bool elsewhere: folder !== "" && paneUri !== "" && folder.replace(/\/+$/, "") !== paneUri.replace(/\/+$/, "")
    /// The file on this machine: the URI itself for a local file, the fetched copy for one on a
    /// server, "" until there is one. What the kind component is handed.
    property string localUri: ""
    /// The fetch in flight: the copy job's id, 0 when none; the job as Jobs last reported it;
    /// where the copy will be once it is done.
    property int job: 0
    property var jobNow: null
    property string jobPath: ""
    property string error: ""
    /// A remote file over the size limit waits to be asked for; `fetchAnyway` is the answer.
    property bool offered: false
    property bool fetchAnyway: false

    /// The shell moves the selection by this much; the row that lands is shown here.
    signal step(int delta)
    /// The unsupported face's button: the shell's own Open with menu, aimed at the selection.
    signal openWith()
    /// The window went down — by its close box, Esc, Space, or the compositor. Not `closed`:
    /// that is the window's own, Quickshell's, and fires only for the compositor's kind.
    signal dismissed()

    /// Which component shows this kind: a Markdown document by its name, the rest by the
    /// daemon's kind. "" is the unsupported face. A name rather than a Component so a kind
    /// whose file is not there yet falls to the face (the Loader says Error) instead of
    /// breaking the window.
    function componentFor(kind, name) {
        if (/\.(md|markdown)$/i.test(name || "")) return "QuickLookMarkdown.qml"
        switch (kind) {
        case "image": return "QuickLookImage.qml"
        case "video": return "QuickLookVideo.qml"
        case "pdf": return "QuickLookPdf.qml"
        case "text": case "code": return "QuickLookText.qml"
        }
        return ""
    }
    readonly property string component: componentFor(kind, name)
    /// The kind on show, as `shell state` reports it: image, video, pdf, markdown, text, other.
    readonly property string shown: component === "" ? "other" : component.replace(/^QuickLook|\.qml$/g, "").toLowerCase()
    /// Which face the body wears: content, fetching, offer, error, unsupported.
    readonly property string face: error !== "" ? "error" : (offered ? "offer" : (job ? "fetching" : ((component === "" || body.status === Loader.Error) ? "unsupported" : "content")))

    // The file's name first, as editors title their windows, then the product's name for the
    // window kind — a literal like "kiki", not a catalog word, so a compositor rule (float and
    // pin it above the rest; Settings › Omarchy installs one) matches it in every language.
    title: name + " — Quick Look"
    color: Kiki.Theme.bg
    visible: false

    // ---------------------------------------------------------------- size
    readonly property int defaultWidth: 900
    readonly property int defaultHeight: 650
    readonly property int headerHeight: 34
    /// What the window was last asked to be, so a drag by the user can be told from what it
    /// chose for itself.
    property int askedW: 0
    property int askedH: 0
    /// Whether the picture in it has had the window sized to it: once per opening, not on
    /// every step through a folder of photographs — a window changing size under the pointer
    /// at every j is not something to look at.
    property bool fitted: false
    function remembered() {
        const s = Kiki.Settings.view.quickLookSize
        return s && s.width > 0 && s.height > 0 ? { width: s.width, height: s.height } : { width: defaultWidth, height: defaultHeight }
    }
    /// Four-fifths of the screen, the most the window opens at; no cap when no screen has said.
    function screenCap() {
        const sc = ql.screen
        return { width: sc && sc.width > 0 ? Math.floor(sc.width * 0.8) : 1e9, height: sc && sc.height > 0 ? Math.floor(sc.height * 0.8) : 1e9 }
    }
    /// The implicit size is the one to set: Quickshell sizes the window from it, mapped or
    /// not, and deprecates setting `width` outright.
    function resize(w, h) { askedW = w; askedH = h; implicitWidth = w; implicitHeight = h }
    function sizeFor() {
        const r = remembered(), cap = screenCap()
        resize(Math.min(r.width, cap.width), Math.min(r.height, cap.height))
    }
    /// A picture opens no bigger than it is: once its size is known the window shrinks to it —
    /// never past the size remembered, never past four-fifths of the screen, never so small the
    /// header has no room.
    function fitPicture(w, h) {
        if (!visible || fitted || w <= 0 || h <= 0 || shown !== "image") return
        fitted = true
        const r = remembered(), cap = screenCap()
        const inset = body.item && body.item.inset !== undefined ? body.item.inset : 0
        resize(Math.max(320, Math.min(r.width, cap.width, w + 2 * inset)),
               Math.max(240, Math.min(r.height, cap.height, h + 2 * inset + headerHeight)))
    }
    /// What the window was dragged to is what it opens at next time. The size it chose for a
    /// picture is its own doing and is not.
    function rememberSize() {
        if (askedW > 0 && width > 0 && height > 0 && (width !== askedW || height !== askedH)) Kiki.Settings.set("view", "quickLookSize", { width: width, height: height })
    }

    // ---------------------------------------------------------------- showing
    /// A look leaves nothing: the copy a server's file was fetched to goes when the look is
    /// over — the window closed, or moved on to another file. The daemon removes the copy and
    /// the folder of its moment, and nothing but a fetched copy (owner, 2026-09-25: "quicklook
    /// cleans up after itself I assume?").
    function dropCopy() {
        if (jobPath === "") return
        const p = jobPath; jobPath = ""
        d().request("QuickLookDrop", { path: p }, () => {})
    }
    function show(r, u) {
        if (job) { Kiki.Jobs.cancel(job); job = 0 }
        dropCopy()
        row = r || null; uri = u || ""
        jobNow = null; jobPath = ""; error = ""; offered = false; fetchAnyway = false
        localUri = isLocal ? uri : ""
        if (!visible) { fitted = false; sizeFor(); visible = true }
        if (!isLocal) fetch()
        content.forceActiveFocus()
    }
    function close() { if (visible) visible = false }
    // Everything a close does hangs off `visible` going false, so the compositor closing the
    // window (Quickshell hides it and says so here) is the same close as Esc: the fetch is
    // stopped, the size kept, the picture let go rather than kept decoded behind nothing.
    onVisibleChanged: {
        if (visible) return
        if (job) { Kiki.Jobs.cancel(job); job = 0 }
        dropCopy()
        rememberSize()
        localUri = ""
        dismissed()
    }
    /// The keys the window answers itself, before the kind component's own. Called by the
    /// handler and by tests, which cannot send a key into a window that is not theirs.
    function handleKey(key, modifiers) {
        switch (key) {
        case Qt.Key_Escape: case Qt.Key_Space: close(); return true
        case Qt.Key_J: case Qt.Key_Down: case Qt.Key_Right: step(1); return true
        case Qt.Key_K: case Qt.Key_Up: case Qt.Key_Left: step(-1); return true
        }
        return false
    }

    // ---------------------------------------------------------------- remote files
    /// The setting is in megabytes (`[view] quickLookFetchLimit = 200`), 200 when nothing says.
    readonly property real fetchLimitBytes: (Kiki.Settings.view.quickLookFetchLimit > 0 ? Kiki.Settings.view.quickLookFetchLimit : 200) * 1024 * 1024
    function pathUri(p) { return "file://" + encodeURIComponent(p).replace(/%2F/g, "/") }
    /// A file on a server comes through `QuickLookFetch`: a copy job into the open cache, not
    /// watched for saves, answered with where the copy will be. A big one is offered first.
    function fetch() {
        if (!fetchAnyway && size > fetchLimitBytes) { offered = true; return }
        offered = false
        const u = uri
        d().request("QuickLookFetch", { uri: u }, (ok, err) => {
            // Moved on before it answered — or the window itself gone (the daemon holds this
            // closure until it answers; after a close the id it names is null).
            if (!ql || u !== ql.uri || !ql.visible) return
            if (err) { ql.error = err.message; return }
            if (!ok.job) { ql.localUri = ql.pathUri(ok.path); return }
            ql.jobPath = ok.path || ""
            ql.job = ok.job
            ql.followJob()
        })
    }
    function fetchNow() { fetchAnyway = true; fetch() }
    /// What the kind component is told about the file, in the order it needs it.
    function feed() {
        const it = body.item
        if (!it) return
        if ("daemon" in it) it.daemon = ql.daemon
        it.row = ql.row; it.name = ql.name
        it.uri = ql.localUri
    }
    onLocalUriChanged: feed()
    onRowChanged: feed()
    /// The copy job as the daemon reports it, watched until it is over: done is the copy,
    /// anything else is said in its place.
    function followJob() {
        if (!job) return
        const j = Kiki.Jobs.list.find(x => x.id === ql.job)
        if (!j) return
        jobNow = j
        if (Kiki.Jobs.live(j)) return
        job = 0
        if (j.state === "done") localUri = pathUri(jobPath)
        else error = Kiki.Jobs.completion(j)
    }
    Connections { target: Kiki.Jobs; function onChanged() { ql.followJob() } }
    function progressText() {
        const j = jobNow
        if (!j || !j.bytesTotal) return Kiki.T.tr("quicklook.fetchingWait")
        return Kiki.T.tr("quicklook.fetching", { done: Kiki.Format.transferSize(j.bytes), total: Kiki.Format.transferSize(j.bytesTotal) })
    }

    FocusScope {
        id: content
        anchors.fill: parent
        focus: true
        // What the kind component did not take: closing, and stepping the selection.
        Keys.onPressed: event => { if (ql.handleKey(event.key, event.modifiers)) event.accepted = true }

        // A thin header: the name, where the file is when that is not the pane's folder, the
        // close box. The title bar says the name too, but a tiled window may have none.
        Rectangle {
            id: header
            width: parent.width; height: ql.headerHeight
            color: Kiki.Theme.bgDark
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
            Row {
                anchors.left: parent.left; anchors.leftMargin: 12
                anchors.right: closeBox.left; anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 10
                Text {
                    id: nameText
                    objectName: "quicklook-name"
                    anchors.verticalCenter: parent.verticalCenter
                    // The name gives way to the place before it gives way to nothing: both are
                    // measured, and the name takes what the place leaves.
                    width: Math.min(implicitWidth, parent.width - (where.visible ? where.width + parent.spacing : 0))
                    elide: Text.ElideMiddle
                    text: ql.name; color: Kiki.Theme.fg
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true
                }
                Text {
                    id: where
                    objectName: "quicklook-where"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: ql.elsewhere
                    width: Math.min(implicitWidth, Math.max(0, parent.width * 0.6))
                    elide: Text.ElideMiddle
                    text: Kiki.T.tr("quicklook.in", { folder: Kiki.Format.display(ql.folder, ql.home) })
                    color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
                }
            }
            ToggleButton {
                id: closeBox
                objectName: "quicklook-close"
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                icon: "x"; tip: Kiki.T.tr("quicklook.close")
                onClicked: ql.close()
            }
        }

        Item {
            id: stage
            y: header.height; width: parent.width; height: Math.max(0, parent.height - header.height)

            // The kind's own component, with the window's content area to itself. Handed the
            // file on this machine, and — the ones that read through the daemon — the daemon a
            // test gave the window. Fed again whenever what is shown changes: stepping from one
            // picture to the next keeps the component and changes what it shows. One function,
            // daemon first and the URI last, rather than Bindings: a Binding on `uri` fires on
            // the Loader's status before `loaded` does, and the component read the file through
            // a daemon it had not been given yet.
            Loader {
                id: body
                objectName: "quicklook-body"
                anchors.fill: parent
                focus: true
                active: ql.visible && ql.localUri !== "" && ql.component !== ""
                source: ql.component
                onLoaded: { ql.feed(); item.forceActiveFocus() }
            }
            // The picture says how big it is once decoded; the window sizes itself to it.
            Connections {
                target: body.item
                ignoreUnknownSignals: true
                function onNaturalChanged() { ql.fitPicture(body.item.natural.width, body.item.natural.height) }
            }

            // What the window cannot show: the file as the list knows it, and the way to an
            // application. Also a kind component that is not there yet.
            Column {
                objectName: "quicklook-unsupported"
                visible: ql.face === "unsupported"
                anchors.centerIn: parent
                width: Math.min(parent.width - 48, 420); spacing: 14
                KindIcon { anchors.horizontalCenter: parent.horizontalCenter; kind: ql.kind; size: 96; color: Kiki.Theme.kindColor(ql.kind) }
                Text {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideMiddle
                    text: ql.name; color: Kiki.Theme.fg
                    font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true
                }
                Text {
                    objectName: "quicklook-kind-size"
                    width: parent.width; horizontalAlignment: Text.AlignHCenter
                    text: ql.row && ql.row.isDir ? Kiki.Format.kindLabel(ql.kind) : Kiki.T.tr("quicklook.kindSize", { kind: Kiki.Format.kindLabel(ql.kind), size: Kiki.Format.bytes(ql.size) })
                    color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                }
                Text {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
                    text: Kiki.T.tr("quicklook.cannotShow"); color: Kiki.Theme.muted
                    font.family: Kiki.Theme.mono; font.pixelSize: 11
                }
                Button { objectName: "quicklook-open-with"; anchors.horizontalCenter: parent.horizontalCenter; text: Kiki.T.tr("quicklook.openWith"); onClicked: ql.openWith() }
            }

            // A file on its way from a server: how far it has come, and the way out.
            Column {
                objectName: "quicklook-fetching"
                visible: ql.face === "fetching"
                anchors.centerIn: parent
                width: Math.min(parent.width - 48, 360); spacing: 12
                readonly property real fraction: ql.jobNow ? Kiki.Jobs.fraction(ql.jobNow) : 0
                KindIcon { anchors.horizontalCenter: parent.horizontalCenter; kind: ql.kind; size: 64; color: Kiki.Theme.kindColor(ql.kind) }
                Text {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideMiddle
                    text: ql.name; color: Kiki.Theme.fg
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true
                }
                Rectangle {
                    width: parent.width; height: 4; radius: 2; color: Kiki.Theme.surface
                    Rectangle { width: Math.round(parent.width * parent.parent.fraction); height: parent.height; radius: 2; color: Kiki.Theme.accent }
                }
                Text {
                    objectName: "quicklook-progress"
                    width: parent.width; horizontalAlignment: Text.AlignHCenter
                    text: ql.progressText(); color: Kiki.Theme.fgDim
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                }
                Text {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter
                    text: Kiki.T.tr("quicklook.escCancels"); color: Kiki.Theme.muted
                    font.family: Kiki.Theme.mono; font.pixelSize: 11
                }
            }

            // Over the limit: how big it is, and the fetch as a choice rather than a fact.
            Column {
                objectName: "quicklook-offer"
                visible: ql.face === "offer"
                anchors.centerIn: parent
                width: Math.min(parent.width - 48, 420); spacing: 14
                KindIcon { anchors.horizontalCenter: parent.horizontalCenter; kind: ql.kind; size: 96; color: Kiki.Theme.kindColor(ql.kind) }
                Text {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideMiddle
                    text: ql.name; color: Kiki.Theme.fg
                    font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true
                }
                Text {
                    objectName: "quicklook-too-big"
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
                    text: Kiki.T.tr("quicklook.tooBig", { size: Kiki.Format.bytes(ql.size) }); color: Kiki.Theme.fgDim
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                }
                Button { objectName: "quicklook-fetch-anyway"; anchors.horizontalCenter: parent.horizontalCenter; text: Kiki.T.tr("quicklook.fetchAnyway"); onClicked: ql.fetchNow() }
            }

            // The daemon said no — a fetch that failed, a file that vanished — in its own words,
            // already in the window's language.
            Column {
                objectName: "quicklook-error"
                visible: ql.face === "error"
                anchors.centerIn: parent
                width: Math.min(parent.width - 48, 420); spacing: 14
                KindIcon { anchors.horizontalCenter: parent.horizontalCenter; kind: ql.kind; size: 64; color: Kiki.Theme.kindColor(ql.kind) }
                Text {
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideMiddle
                    text: ql.name; color: Kiki.Theme.fg
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true
                }
                Text {
                    objectName: "quicklook-error-text"
                    width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
                    text: ql.error; color: Kiki.Theme.danger
                    font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                }
            }
        }
    }
}
