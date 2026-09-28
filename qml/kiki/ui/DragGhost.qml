import QtQuick
import ".." as Kiki

// The drag's image. A drag under Wayland shows whatever the source hands the compositor, and
// kiki handed nothing: the person saw the compositor's hand and no sign of what they held
// (owner, 2026-09-28: "just see a hand now"). This holds one FileFan off screen, fills it at
// the press — before the drag threshold is crossed, so the grab has landed by the time the drag
// starts — and keeps the grabbed image's url for the view's `Drag.imageSource`. Off screen and
// not hidden, because a grab needs an item that renders; one for the window, reached from any
// view through its pane (`Kiki.Pane.ghost`), which is what every view already holds.
Item {
    id: ghost
    width: fan.width; height: fan.height
    x: -10000; y: -10000

    property alias fan: fan
    /// The grabbed image, or "" until the grab has landed.
    property url url: ""
    property bool ready: false
    property int _generation: 0

    FileFan { id: fan; badgeShown: false }
    // The first grab of a session is slow — shaders, a framebuffer — and a drag that started
    // before it landed went without a picture, while the next one had the previous grab to use
    // (owner, 2026-09-28: "dragging first time there is no image"). Warm it once at start.
    Component.onCompleted: { fan.rows = [{ kind: "file", thumb: "" }]; fan.count = 1; regrab() }
    /// Run `cb` once a picture is ready — now, or when the grab lands, or after `cap` ms without
    /// one (the drag then goes without a picture rather than not at all).
    property var _waiting: []
    function whenReady(cb, cap) {
        if (ready) { cb(); return }
        const entry = { cb: cb, done: false }
        _waiting.push(entry)
        readyTimer.interval = cap || 200; readyTimer.restart()
    }
    onReadyChanged: if (ready) _flush()
    function _flush() { const w = _waiting; _waiting = []; readyTimer.stop(); for (const e of w) if (!e.done) { e.done = true; e.cb() } }
    property Timer readyTimer: Timer { onTriggered: ghost._flush() }
    /// Start `proxy`'s drag, now, with whatever picture is ready. Never later: a Wayland drag
    /// can only begin inside the pointer event that holds the grab, and a start deferred by even
    /// a frame to wait for the picture began nothing at all (2026-09-28). The picture is made
    /// ready BEFORE the press instead — `prepare` runs when the pointer enters a row.
    function startDrag(proxy, stillHeld) {
        // `begin` before `active`, and no `end` here: on this Qt setting `active` returns at
        // once and the drag runs on its own, so an `end` after it cleared `dragging` before
        // the first move — and the window's badge never showed (2026-09-28, read from a log
        // of a real drag). The view ends it when the button comes up.
        begin(proxy)
        proxy.Drag.imageSource = imageFor(); proxy.Drag.hotSpot = hotSpot
        proxy.Drag.active = true
    }
    /// The pointer is over `index` of `pane`: have its picture ready for a press that may come.
    /// Nothing is re-made when it is the same rows already drawn.
    property string _drawnFor: ""
    function hover(pane, index, ctrl) {
        if (!pane || !pane.listing) return
        const at = pane.selection.has(index) ? pane.selection.positions() : [index]
        const key = pane.uri + "|" + at.join(",") + "|" + (ctrl ? 1 : 0) + "|" + (pane.isLocal ? 0 : 1)
        if (key === _drawnFor && (ready || direct)) return
        _drawnFor = key
        prepare(pane, index, ctrl)
    }

    /// What a press on `index` of `pane` would drag: the selection when the row is in it, the
    /// row alone when not — the same rule as `Pane.dragUris`. Copy when the pane is on a server
    /// (a drag out of it is a download) or Ctrl is held; the drop may still change the action,
    /// but the picture at pick-up says what the hand intends.
    /// Where the drag was picked up, in window coordinates: a drop that comes to nothing flies
    /// the fan back here (owner, 2026-09-28). (-1, -1) until a press has said.
    property point origin: Qt.point(-1, -1)
    /// The picture flies from where the drag was let go back to where it began.
    signal snapBack(point from, point to, var rows, int count)
    /// Qt's word on how the drag ended (`Drag.dragFinished`): a refused or cancelled drop is
    /// IgnoreAction, and then nothing has moved — so the picture goes back where it came from.
    function finished(action) {
        if (action === Qt.IgnoreAction && origin.x >= 0) snapBack(Kiki.DragTrack.pointer, origin, fan.rows, fan.count)
        origin = Qt.point(-1, -1)
    }
    Connections { target: ghost.proxy ? ghost.proxy.Drag : null; function onDragFinished(action) { ghost.finished(action) } }
    function prepare(pane, index, ctrl, pressAt) {
        if (pressAt !== undefined) origin = pressAt
        if (!pane || !pane.listing) return
        const at0 = pane.selection.has(index) ? pane.selection.positions() : [index]
        _drawnFor = pane.uri + "|" + at0.join(",") + "|" + (ctrl ? 1 : 0) + "|" + (pane.isLocal ? 0 : 1)
        const at = pane.selection.has(index) ? pane.selection.positions() : [index]
        const rows = at.slice(0, 3).map(i => { const r = pane.listing.row(i); return r ? { kind: r.kind, thumb: r.thumb || "" } : { kind: "file", thumb: "" } })
        prepareRows(rows, at.length, !!ctrl || !pane.isLocal)
    }
    /// The same, from rows a view already has in hand (the columns view's rows belong to
    /// folders the pane is not standing in).
    function prepareRows(rows, count, copy, pressAt) {
        if (pressAt !== undefined) origin = pressAt
        fan.rows = rows; fan.count = count; fan.badge = copy ? "+" : ""
        copyByDefault = copy
        // One file that has a thumbnail is carried as that thumbnail itself — the cached PNG,
        // no rendering at all (owner, 2026-09-28: "can't we use the thumbnail that already
        // exists?"). A fan, a count, or a file with only an icon is a composition, and rendered.
        if (count === 1 && rows.length === 1 && rows[0].thumb) {
            _generation++
            url = "file://" + rows[0].thumb; direct = true; ready = true
            return
        }
        direct = false
        regrab()
    }
    /// Whether the picture is a thumbnail file rather than a render of the fan.
    property bool direct: false
    function regrab() {
        ready = false
        const gen = ++_generation
        fan.grabToImage(result => {
            if (gen !== _generation) return
            ghost.url = result.url; ghost.ready = true
        })
    }

    // ---------------------------------------------------------------- while the drag is on
    /// What the pick-up meant without a modifier: a copy out of a server, a move otherwise.
    property bool copyByDefault: false
    /// The proxy carrying the drag, from `begin` to `end`: the modifiers can change the picture
    /// while it is held (owner, 2026-09-28: "when I press the modifier keys, the drag image
    /// should show the different actions").
    property Item proxy: null
    readonly property bool dragging: proxy !== null
    function begin(p) { proxy = p }
    function end() { proxy = null }
    // No rebadging by keys: the window never sees them during a drag. The badge the window
    // draws follows `DragTrack.action`, which Qt sets from the modifiers on every move.
    /// The image for a drag that starts now, or "" when the grab has not landed (the drag then
    /// goes without one, as it always did).
    function imageFor() { return ready ? url : "" }
    /// The picture hangs a little below and to the right of the cursor — not centred on it —
    /// so the cursor's tip, and the badge the window draws beside it, stay clear of the image
    /// (owner, 2026-09-28: the "+" was "a bit behind the image").
    readonly property point hotSpot: Qt.point(-18, -18)
}
