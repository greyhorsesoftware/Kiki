import QtQuick
import "." as Kiki

// A virtualised view of one listing: `count` sizes the view, `row(i)` serves a
// delegate, and the cache asks the daemon only for rows near the viewport.
QtObject {
    id: cache

    property string uri: ""
    property int lid: 0
    property int count: 0
    property bool done: false
    property bool cached: false
    property string error: ""

    // Viewport reported by the view, in rows.
    property int viewportFirst: 0
    property int viewportCount: 60
    property int padAhead: 200
    property int padBehind: 100
    property int maxRequest: 512

    property var _rows: ({})        // position -> row object
    property int _reqFirst: -1      // range covered by the last request
    property int _reqEnd: -1
    property int _lastDirection: 1
    // Resolved lazily so a test can inject a fake without the real singleton (and its Quickshell
    // socket) ever being instantiated.
    property var daemon: null
    function d() { return daemon || Kiki.Daemon }

    signal reset()
    signal rowsUpdated(int first, int n)

    function row(i) { return _rows[i] || null }

    function open(newUri) {
        close()
        uri = newUri
        lid = d().allocLid()
        d().bind(lid, cache)
        _rows = ({}); count = 0; done = false; error = ""; _reqFirst = -1; _reqEnd = -1
        // Open and the first Window leave in one write.
        d().request("Open", { lid: lid, uri: uri }, (ok, err) => {
            if (err) { error = err.message; return }
            cached = ok.cached
        })
        _request(0, Math.min(viewportCount + padAhead, maxRequest))
    }

    function close() {
        if (lid) { d().request("Close", { lid: lid }); d().unbind(lid); lid = 0 }
    }

    function setViewport(first, n) {
        _lastDirection = first >= viewportFirst ? 1 : -1
        viewportFirst = first; viewportCount = n
        debounce.restart()
    }

    function sort(role, order) { d().request("Sort", { lid: lid, role: role, order: order }) }
    function filter(text) { d().request("Filter", { lid: lid, text: text }) }
    function showHidden(show) { d().request("ShowHidden", { lid: lid, show: show }) }
    function refresh() { d().request("Refresh", { lid: lid }) }

    function _wanted() {
        const ahead = _lastDirection > 0 ? padAhead : padBehind
        const behind = _lastDirection > 0 ? padBehind : padAhead
        let first = Math.max(0, viewportFirst - behind)
        let end = Math.min(count > 0 ? count : viewportFirst + viewportCount + ahead, viewportFirst + viewportCount + ahead)
        return [first, end]
    }

    function _request(first, n) {
        if (!lid || n <= 0) return
        _reqFirst = first; _reqEnd = first + n
        d().request("Window", { lid: lid, first: first, count: n }, (ok, err) => {
            if (err) { error = err.message; return }
            _apply(ok.first, ok.rows)
            count = ok.n; done = ok.done
        })
    }

    function _fill() {
        const [first, end] = _wanted()
        // Only ask for rows we do not hold.
        let a = first
        while (a < end && _rows[a]) a++
        let b = end
        while (b > a && _rows[b - 1]) b--
        if (b - a <= 0) return
        // Hysteresis: if every visible row is held and only a sliver of padding is missing, wait
        // for the scroll to move further rather than sending a tiny request per pixel.
        let visibleHeld = true
        for (let i = viewportFirst; i < Math.min(end, viewportFirst + viewportCount); i++) if (!_rows[i]) { visibleHeld = false; break }
        if (visibleHeld && b - a < Math.max(1, Math.floor(Math.max(padAhead, padBehind) / 2))) return
        _request(a, Math.min(b - a, maxRequest))
    }

    function _apply(first, rows) {
        for (let i = 0; i < rows.length; i++) _rows[first + i] = rows[i]
        // Drop rows far outside the window so a long scroll does not keep everything.
        const keepFrom = viewportFirst - 4 * padBehind, keepTo = viewportFirst + viewportCount + 4 * padAhead
        for (const k in _rows) { const p = Number(k); if (p < keepFrom || p > keepTo) delete _rows[k] }
        rowsUpdated(first, rows.length)
    }

    function handleEvent(msg) {
        switch (msg.event) {
        case "Count":
            count = msg.n; done = msg.done
            break
        case "Rows":
            _apply(msg.first, msg.rows)
            break
        case "Reset":
            _rows = ({}); count = msg.n; _reqFirst = -1; _reqEnd = -1
            reset()
            _fill()
            break
        case "Gone":
            error = "gone"; _rows = ({}); count = 0
            reset()
            break
        }
    }

    property Timer debounce: Timer {
        interval: 16
        repeat: false
        onTriggered: cache._fill()
    }
}
