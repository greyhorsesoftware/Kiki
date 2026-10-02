import QtQuick

// Opens a folder in a pane and says what the opening cost, from the window's side of the
// socket: when the first rows were there to draw, when the first frame after that was drawn,
// when their sizes and dates had landed, when the count was final — and how many `Window`
// requests the opening took, which is the number docs/0.5.0/10-faster-listings.md exists to
// bring to nought. The open_perf flow reads this through the shell's IPC; nothing in the window
// depends on it.
Item {
    id: probe
    visible: false

    property var pane: null
    property var cache: null
    property bool running: false
    property var result: ({})

    property double _t0: 0
    property int _serial0: 0
    property double _firstRows: 0
    property double _firstFrame: 0
    property double _firstMeta: 0
    property double _done: 0
    property int _requestsAtRows: 0

    /// Open `uri` in `p` and watch its listing until the first screenful is whole, or 10 s.
    function start(p, uri) {
        if (!p || !p.listing) { result = { error: "no pane to open in" }; return }
        pane = p; cache = p.listing
        _firstRows = 0; _firstFrame = 0; _firstMeta = 0; _done = 0; _requestsAtRows = 0
        _serial0 = cache._serial
        cache.resetCost()
        result = { running: true }
        running = true
        _t0 = Date.now()
        // Not watched while `open` runs: it clears the cache a property at a time, and the
        // count going to nought while `done` was still the old folder's read as an empty folder.
        _armed = false
        pane.open(uri)
        _armed = true
        _look()
    }
    property bool _armed: false

    function _screen() { return Math.min(cache.count, cache.viewportFirst + cache.viewportCount) }
    function _rowsThere() { const n = _screen(); if (n <= 0) return false; for (let i = cache.viewportFirst; i < n; i++) if (!cache.row(i)) return false; return true }
    function _metaThere() { const n = _screen(); if (n <= 0) return false; for (let i = cache.viewportFirst; i < n; i++) { const r = cache.row(i); if (!r || !r.meta) return false } return true }

    function _look() {
        if (!running || !_armed) return
        const now = Date.now()
        if (_firstRows === 0 && _rowsThere()) { _firstRows = now; _requestsAtRows = cache._serial - _serial0 }
        if (_firstMeta === 0 && _metaThere()) _firstMeta = now
        if (_done === 0 && cache.done) _done = now
        // An empty folder has nothing to wait for but its count.
        if (cache.done && cache.count === 0) { _firstRows = _firstRows || now; _firstMeta = _firstMeta || now; _firstFrame = _firstFrame || now }
        if (_firstRows && _firstFrame && _firstMeta && _done || now - _t0 > 10000) _finish(now)
    }

    function _finish(now) {
        running = false
        const ms = t => t ? t - _t0 : -1
        result = {
            running: false, uri: cache.uri, rows: cache.count, cached: cache.cached, error: cache.error,
            firstRowsMs: ms(_firstRows), firstFrameMs: ms(_firstFrame), firstMetaMs: ms(_firstMeta), doneMs: ms(_done),
            requestsAtRows: _requestsAtRows, requests: cache._serial - _serial0,
            // Where the window's own time went: the scan's Count events, every event's handling,
            // and the delegates the view built for this folder (docs/0.5.0/05-window-memory.md).
            countEvents: cache._countEvents, eventMs: Math.round(cache._msEvents * 10) / 10, delegates: cache._delegates,
            timedOut: now - _t0 > 10000
        }
    }

    Connections {
        target: probe.cache
        function onRowsUpdated(first, n) { probe._look() }
        function onReset() { probe._look() }
        function onCountChanged() { probe._look() }
        function onDoneChanged() { probe._look() }
    }
    // A frame drawn after the rows were there: the nearest the window can come to "painted".
    FrameAnimation {
        running: probe.running
        onTriggered: { if (probe._firstRows && probe._firstFrame === 0) probe._firstFrame = Date.now(); probe._look() }
    }
}
