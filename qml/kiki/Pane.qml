import QtQuick
import "." as Kiki

// A pane is a view of one URI: its listing, history, view mode, sort and selection.
QtObject {
    id: pane
    property string uri: ""
    property string view: "list"          // icon | list | columns
    property string sortRole: "name"
    property string sortOrder: "asc"
    property string filterText: ""
    // Dot-files: starts from the setting, toggled per pane with Ctrl+H or the view menu.
    property bool showHidden: Kiki.Settings.view.showHidden === true
    function setHidden(show) { showHidden = show; listing.showHidden(show); _remember() }
    property var history: []
    property int historyIndex: -1
    property bool focused: false
    // Icon view zoom: pinch on a trackpad, or Ctrl and the wheel.
    property real iconZoom: 1
    property int renamingIndex: -1
    signal renameRequested(string uri, string name)

    property Kiki.WindowCache listing: Kiki.WindowCache {}
    property Kiki.Selection selection: Kiki.Selection {}

    signal navigated(string uri)

    /// Coming back out of a folder, the folder we left is the one worth selecting. The shell
    /// looks up its row once the listing has finished.
    property var visited: ({})        // folder uri -> the row that was selected there
    property string selectAfterLoad: ""
    /// Set by the keyboard paths into a folder: land on its first row so the arrows carry on.
    property bool selectFirstAfterLoad: false
    function open(target, push) {
        if (push === undefined) push = true
        const leaving = trimUri(uri)
        const dest = trimUri(target)
        // Remember what was selected here, so coming back lands on it again.
        const cur = selection.current
        const curRow = cur >= 0 ? listing.row(cur) : null
        if (leaving && curRow) { const v = Object.assign({}, visited); v[leaving] = curRow.name; visited = v }
        const child = leaving && parentOf(leaving) === dest ? decodeURIComponent(leaving.split("/").pop()) : ""
        selectAfterLoad = child || visited[dest] || ""
        selectFirstAfterLoad = false
        if (push) {
            history = history.slice(0, historyIndex + 1).concat([target])
            historyIndex = history.length - 1
        }
        uri = target
        filterText = ""
        selection.clear()
        // Per-folder memory (plan 02): restore this folder's view and sort, else keep the current ones.
        _applyPref(target)
        listing.open(target)
        // Always state the order: the daemon caches listings, so this folder may still carry the
        // order some earlier pane asked for. The daemon ignores a sort that is already in force.
        listing.sort(sortRole, sortOrder)
        if (showHidden !== (Kiki.Settings.view.showHidden === true)) listing.showHidden(showHidden)
        navigated(target)
    }
    /// Whether this pane reads and writes per-folder view memory. Off side by side: a folder
    /// remembered as Gallery must not open that way in half a window, and nothing chosen there —
    /// view, sort, hidden files — is the folder's preference. One switch for read and write, so
    /// the two cannot drift apart.
    property bool rememberViews: true
    /// Put this folder back the way it is remembered: for the pane that stays when side by side
    /// is turned off.
    function applyPref() {
        if (!uri) return
        const was = [sortRole, sortOrder, showHidden]
        _applyPref(uri)
        if (was[0] !== sortRole || was[1] !== sortOrder) listing.sort(sortRole, sortOrder)
        if (was[2] !== showHidden) listing.showHidden(showHidden)
        _smartChecked = ""; if (listing.done) _smart()
    }
    function _applyPref(target) {
        if (!rememberViews) { hasPref = false; return }     // keep whatever this pane is showing
        const pref = Kiki.Settings.viewPref(target.replace(/\/+$/, "") || target)
        hasPref = !!pref
        _applying = true
        // "mirror" was once a view (the two-pane layout lived in `Pane.view`), so old entries in
        // views.toml can still say it. Nobody chose that; it is read as no view at all.
        const stored = pref && pref.view !== "mirror" ? pref.view : ""
        if (pref) { if (stored && stored !== view) view = stored; else if (!stored && view === "mirror") view = "list"; sortRole = pref.sort || "name"; sortOrder = pref.order || "asc"; showHidden = pref.hidden !== undefined ? pref.hidden : (Kiki.Settings.view.showHidden === true) }
        else {
            showHidden = Kiki.Settings.view.showHidden === true
            const d = Kiki.Settings.view["default"]
            const want = !d || d === "mirror" ? "list" : d
            if (view !== want) view = want
        }
        _applying = false
    }
    property bool _applying: false
    property bool hasPref: false
    // Smart default (plan 24's rule, plan 27's answer): with no memory for this folder, a picture
    // folder — by name, or by holding mostly pictures — opens in GALLERY, not icon view. Plan 24
    // still says icon; the code is right and the plan is amended (D18). `tst_PaneSmartView` pins it.
    property string _smartChecked: ""
    readonly property var pictureNames: ["pictures", "photos", "dcim", "screenshots", "wallpapers", "camera", "camera roll"]
    function _smart() {
        if (_smartChecked === uri || hasPref || !rememberViews) return
        _smartChecked = uri
        const name = decodeURIComponent(uri.replace(/\/+$/, "").split("/").pop() || "").toLowerCase()
        let pick = pictureNames.indexOf(name) >= 0 ? "gallery" : ""
        if (!pick) {
            const n = Math.min(listing.count, 200); let held = 0, media = 0
            for (let i = 0; i < n; i++) { const r = listing.row(i); if (!r) continue; held++; if (r.kind === "image" || r.kind === "video") media++ }
            if (held >= 12 && media / held >= 0.6) pick = "gallery"
        }
        if (pick && pick !== view) { _applying = true; view = pick; _applying = false }
    }
    property Connections doneWatch: Connections { target: listing; function onDoneChanged() { if (listing.done) _smart() } }

    // Keeping the selection across a refresh. A job finishing, or anything else touching the
    // folder, makes the daemon re-send the listing, which clears the selection — so the file you
    // were about to rename is suddenly nothing. Remember the names and put them back.
    property var _keepNames: []
    property string _keepUri: ""
    property bool _restoring: false
    property bool _restorePending: false
    function selectedNames() {
        return selection.positions().map(p => { const r = listing.row(p); return r ? r.name : null }).filter(n => n)
    }
    function _restoreSelection() {
        if (!_restorePending) return
        if (listing.uri !== _keepUri || !_keepNames.length) { _restorePending = false; return }
        const want = _keepNames
        let found = []
        for (let i = 0; i < listing.count; i++) { const r = listing.row(i); if (r && want.indexOf(r.name) >= 0) found.push(i) }
        if (!found.length) return                     // rows not back yet; try again on the next batch
        _restoring = true
        selection.set(found[0])
        for (let j = 1; j < found.length; j++) selection.rows[found[j]] = true
        selection.changed()
        _restoring = false
        _restorePending = false
    }
    property Connections selWatch: Connections {
        target: pane.selection
        function onChanged() { if (!pane._restoring) { pane._keepNames = pane.selectedNames(); pane._keepUri = pane.listing.uri } }
    }
    property Connections keepWatch: Connections {
        target: pane.listing
        // A reset from the watcher stays in the same folder; one from navigating does not, and
        // there the selection rules of `open` apply instead.
        function onReset() { pane._restorePending = pane.listing.uri === pane._keepUri && pane._keepNames.length > 0 }
        function onRowsUpdated(first, n) { pane._restoreSelection() }
        // A file arriving or leaving moves the rows under the selection, not the selection.
        function onSpliced(ops) {
            pane.selection.splice(ops)
            let r = pane.renamingIndex
            for (const op of ops) { if (r < 0) break; r = op.op === "remove" ? (r === op.pos ? -1 : r > op.pos ? r - 1 : r) : (r >= op.pos ? r + 1 : r) }
            pane.renamingIndex = r
        }
    }
    function _remember() { if (!_applying && rememberViews && uri) Kiki.Settings.setViewPref(uri.replace(/\/+$/, "") || uri, view, sortRole, sortOrder, showHidden) }
    onViewChanged: _remember()
    function canBack() { return historyIndex > 0 }
    function canForward() { return historyIndex < history.length - 1 }
    function back() { if (canBack()) { historyIndex--; open(history[historyIndex], false) } }
    function forward() { if (canForward()) { historyIndex++; open(history[historyIndex], false) } }
    function up() { const p = parentOf(uri); if (p) open(p) }
    function setSort(role, order) { sortRole = role; sortOrder = order; listing.sort(role, order); _remember() }
    function setFilter(text) { filterText = text; listing.filter(text) }

    function parentOf(u) {
        const i = u.indexOf("://"); const head = u.slice(0, i + 3); let rest = u.slice(i + 3)
        const slash = rest.indexOf("/"); const auth = slash < 0 ? rest : rest.slice(0, slash); let path = slash < 0 ? "/" : rest.slice(slash)
        if (path === "/" || path === "") return null
        path = path.replace(/\/+$/, ""); const k = path.lastIndexOf("/")
        return head + auth + (k <= 0 ? "/" : path.slice(0, k))
    }
    function childUri(name) {
        const base = uri.endsWith("/") ? uri : uri + "/"
        return base + encodeURIComponent(name).replace(/%2F/g, "/")
    }
    /// Drop trailing slashes but keep the authority's own, so "file:///" survives.
    function trimUri(u) {
        const m = u.match(/^([a-z0-9+.-]+:\/\/[^/]*)(\/.*)?$/i)
        if (!m) return u.replace(/\/+$/, "")
        const p = (m[2] || "/").replace(/\/+$/, "")
        return m[1] + (p || "/")
    }
    readonly property bool isTrash: uri.startsWith("trash://")

    // Drag and drop (plan 02). The dragged payload is text/uri-list so drops also work from and
    // into other Wayland apps.
    function dragUris(index) {
        const rows = selection.has(index) ? selection.positions() : [index]
        return rows.map(i => { const r = listing.row(i); return r ? childUri(r.name) : null }).filter(u => u)
    }
    function dragMime(index) { return uriListMime(dragUris(index)) }
    /// What every view drags, in one place: columns builds its own list of URIs — its rows can
    /// belong to a folder this pane is not standing in — but the payload is shaped here.
    function uriListMime(uris) { return { "text/uri-list": uris.join("\r\n") + "\r\n" } }
    /// A drop landed here. The window gives this pane the focus.
    signal received()

    /// Whether the keys held ask for a copy, read off the event Qt really delivers. A DragEvent
    /// has NO `modifiers` (Qt's own metatypes: x, y, source, keys, supportedActions,
    /// proposedAction, action, accepted, …); Qt folds the keys into `proposedAction` instead.
    /// Measured with real in-process drags (tests/qml-drag): no key and Shift → Move, the
    /// source's proposal; Ctrl → Copy — and Alt too, Qt's fallback for a source that offers no
    /// Link. So Copy means a key asked for one; Move is no key OR Shift — they cannot be told
    /// apart — and gets kiki's own rule by place.
    function wantsCopy(ev) { return ev.proposedAction === Qt.CopyAction }
    function urlsOf(ev) {
        return ev.hasUrls ? ev.urls.map(u => u.toString()) : (ev.hasText ? ev.text.split(/\r?\n/).filter(l => l && !l.startsWith("#")) : [])
    }

    /// What letting go here would do, told to the drag while it is still in the air: the cursor
    /// is drawn from it — the theme's copy and move cursors, and "not allowed" over a target that
    /// would refuse, *now* rather than when the button comes up. Read again on every move, so a
    /// key going down changes it on the spot. True when the drop would be taken.
    function dragOver(dest, drag) {
        const action = dropAction(urlsOf(drag), dest, wantsCopy(drag))
        // Taken either way, with "ignore" as the answer when it would refuse — see DropTarget.
        drag.accepted = true
        drag.action = !action ? Qt.IgnoreAction : action.op === "copy" ? Qt.CopyAction : Qt.MoveAction
        return !!action
    }

    // Drop `drop` (a DragEvent) into `dest`: move within one place, copy across, Ctrl copies.
    function dropInto(dest, drop) {
        // One drop, one job. Drop targets lie over each other — a folder row over its view's
        // background, a column over the pane's — and Qt hands the same drop to each of them in
        // turn, topmost first. The second would move the files again: a collision prompt for a
        // file that has already gone ("incoming 0 B"). Whoever accepted it first has dealt with it.
        if (drop.accepted) return
        const action = dropAction(urlsOf(drop), dest, wantsCopy(drop))
        // A refusal is answered, not passed on: "ignore" sends the drag back to where it came
        // from, and marks the drop dealt with, so the folder behind this one — which may well
        // take it — does not quietly do what was just refused.
        if (!action) { drop.accept(Qt.IgnoreAction); return }
        received()
        drop.accept(action.op === "copy" ? Qt.CopyAction : Qt.MoveAction)
        Kiki.Jobs.submit({ op: action.op, items: action.items, dest: dest })
    }
    /// Where a URI lives: its scheme and authority. `sftp://nas` and `sftp://backup` are two
    /// machines, however alike they look; every `file://` is this one.
    function placeOf(u) { const m = /^([a-z][a-z0-9+.-]*):\/\/([^\/]*)/i.exec(u); return m ? (m[1] + "://" + m[2]).toLowerCase() : "" }
    /// What dropping `urls` on the folder `dest` does: `{ op, items }`, or null for nothing.
    ///  - What is already there is not dropped again: an item whose folder IS `dest`, `dest`
    ///    itself, and a folder onto itself or into something inside it.
    ///  - `copy` (Ctrl was held) copies. Otherwise: a MOVE within one place, a COPY between
    ///    places — between machines a move is a copy and then a delete, and nobody asked for the
    ///    delete. Shift cannot force a move across machines: Qt reports it as it reports no key
    ///    at all (`wantsCopy`); cut and paste moves across machines.
    function dropAction(urls, dest, copy) {
        // Never INTO the trash view — not the folder being shown and not a folder inside it: what
        // is in there is on its way out. (The Trash in the sidebar is a different target, and a
        // drop on it goes to `trashSelection` instead of here.)
        if (dest.startsWith("trash:")) return null
        const d = dest.replace(/\/+$/, "")
        const items = urls.filter(u => {
            const s = (u || "").replace(/\/+$/, "")
            return s !== "" && parentOf(s) !== d && s !== d && d.indexOf(s + "/") !== 0
        })
        if (!items.length) return null
        const samePlace = items.every(u => placeOf(u) === placeOf(dest))
        const op = copy || !samePlace ? "copy" : "move"
        return { op: op, items: items }
    }
}
