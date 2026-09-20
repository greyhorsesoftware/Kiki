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
    // Which state of the daemon's view the rows held describe (its `gen`), -1 when not known. A
    // `Window` reply and the event announcing a change travel separately, so either can arrive
    // first; the number is what says which of them is news.
    property int _gen: -1
    // After a `Reset` the rows held are kept on screen until their replacements arrive — most
    // resets (a rescan, a job finishing) bring back the very same rows, and emptying first made
    // every row and thumbnail blink for the length of a round trip.
    property bool _stale: false
    property int _serial: 0         // the latest Window request
    // The request whose answer `_fill` is waiting for, and when it left. One at a time: the next
    // is asked the moment this one lands, for wherever the viewport is by then — which paces the
    // asking to what the daemon can answer instead of to the frame rate.
    property int _pending: 0
    property int _oldReplies: 0
    /// How long an answer has been taking, smoothed. A fling covers ground while one is in the
    /// post, and this is how far.
    property real _rtt: 0
    // Where a round trip's time goes, summed since `resetCost()`. Only the scroll probe reads
    // these; nothing in the window depends on them. `wait` is the daemon, the socket and turning
    // the reply into JavaScript; `store` is writing the rows into the cache; `bind` is every
    // delegate re-reading its row, which happens synchronously inside `rowsUpdated`.
    property real _msWait: 0
    property real _msStore: 0
    property real _msBind: 0
    property int _answers: 0
    function resetCost() { _msWait = 0; _msStore = 0; _msBind = 0; _answers = 0 }
    property double _pendingAt: 0
    // Resolved lazily so a test can inject a fake without the real singleton (and its Quickshell
    // socket) ever being instantiated.
    property var daemon: null
    function d() { return daemon || Kiki.Daemon }

    signal reset()
    signal rowsUpdated(int first, int n)
    /// Rows came or went in place: `ops` is [{ op: "remove" | "insert", pos }], each position as
    /// of the moment of that step. Whoever holds positions (a selection) moves them the same way.
    signal spliced(var ops)

    function row(i) { return _rows[i] || null }

    function open(newUri) {
        close()
        uri = newUri
        lid = d().allocLid()
        d().bind(lid, cache)
        _rows = ({}); count = 0; done = false; error = ""; _reqFirst = -1; _reqEnd = -1; _gen = -1; _stale = false; _pending = 0
        // Open and the first Window leave in one write.
        d().request("Open", { lid: lid, uri: uri }, (ok, err) => {
            // Nothing more is coming: say so, or the pane waits on a folder that never opened.
            if (err) { error = err.message; done = true; return }
            cached = ok.cached
        })
        _request(0, Math.min(viewportCount + padAhead, maxRequest))
    }

    // A cache made on the fly (a column, the project tree) is destroyed, not closed: without this
    // the daemon kept the listing subscribed — never evicted, its folder watched — for the life of
    // the window, and `Daemon._listings` kept a dead object that the next event was dispatched to.
    Component.onDestruction: close()

    function close() {
        if (lid) { d().request("Close", { lid: lid }); d().unbind(lid); lid = 0 }
    }

    // How fast the viewport is moving, in rows a second (smoothed), and how far ahead of it rows
    // are therefore wanted: 200 rows is a comfortable margin for a wheel and 40 ms of a fling.
    property real _velocity: 0
    property double _movedAt: 0
    readonly property real leadSeconds: 0.4
    readonly property int maxAhead: 3000
    function _ahead() { return Math.max(padAhead, Math.min(maxAhead, Math.round(_velocity * leadSeconds))) }

    function setViewport(first, n) {
        const now = Date.now(), dt = now - _movedAt
        if (first !== viewportFirst) {
            // A pause, or a jump (Home, End, a seek), is not a speed.
            const v = dt > 0 && dt < 250 ? Math.abs(first - viewportFirst) * 1000 / dt : 0
            _velocity = v === 0 || (first >= viewportFirst ? 1 : -1) !== _lastDirection ? 0 : _velocity * 0.7 + v * 0.3
            _movedAt = now
        }
        _lastDirection = first >= viewportFirst ? 1 : -1
        viewportFirst = first; viewportCount = n
        // Not `restart()`: a scroll moves the viewport every frame, and a timer put back to zero
        // sixty times a second never fires — nothing was fetched until the scrolling stopped, so
        // a steady scroll through a long folder was a scroll through blank rows.
        if (!debounce.running) debounce.start()
    }

    function sort(role, order) { d().request("Sort", { lid: lid, role: role, order: order }) }
    function filter(text) { d().request("Filter", { lid: lid, text: text }) }
    function showHidden(show) { d().request("ShowHidden", { lid: lid, show: show }) }
    function refresh() { d().request("Refresh", { lid: lid }) }

    function _wanted() {
        // Stopped for a moment: the speed is history.
        if (Date.now() - _movedAt > 250) _velocity = 0
        const ahead = _lastDirection > 0 ? _ahead() : padBehind
        const behind = _lastDirection > 0 ? padBehind : _ahead()
        let first = Math.max(0, viewportFirst - behind)
        let end = Math.min(count > 0 ? count : viewportFirst + viewportCount + ahead, viewportFirst + viewportCount + ahead)
        return [first, end]
    }

    function _request(first, n) {
        if (!lid || n <= 0) return
        _reqFirst = first; _reqEnd = first + n
        const serial = ++_serial
        _pending = serial; _pendingAt = Date.now()
        // `first`/`n` is what to send; `viewFirst`/`viewCount` is what is actually on screen. The
        // daemon makes thumbnails for the latter only — during a scroll the look-ahead runs
        // thousands of rows past the viewport, and a picture for a row nobody is looking at costs
        // a decoder process and buys nothing.
        d().request("Window", { lid: lid, first: first, count: n, viewFirst: viewportFirst, viewCount: viewportCount }, (ok, err) => {
            const awaited = serial === _pending
            if (awaited) {
                const waited = Date.now() - _pendingAt
                _msWait += waited; _answers++
                _rtt = _rtt === 0 ? waited : _rtt * 0.7 + waited * 0.3
                _pending = 0
            }
            // The Window that left with a failed Open only reports "no listing": keep the reason.
            if (err) { if (!error) error = err.message; if (_stale) { _stale = false; _rows = ({}) } return }
            if (ok.gen !== undefined) {
                // Computed before a change we have already been told of: its positions are the old
                // ones. Ask again rather than show them.
                // (Unless something newer has been asked since, which will bring them.)
                // Asked through the timer, never from inside the answer; and a daemon that keeps
                // answering from the past is believed after a few tries rather than asked for ever.
                if (_gen >= 0 && ok.gen < _gen && _oldReplies < 5) { _oldReplies++; if (serial === _serial) { _stale = true; if (!debounce.running) debounce.start() } return }
                _oldReplies = 0
                // Newer than anything held (the event is still on its way): only it can be trusted.
                if (_gen >= 0 && ok.gen > _gen) _stale = true
                _gen = ok.gen
            }
            if (_stale) { _stale = false; _rows = ({}) }
            _apply(ok.first, ok.rows)
            count = ok.n; done = ok.done
            // The viewport may have moved on while this was on its way: ask for what it lacks now,
            // without waiting out the timer — behind a fling, a frame's wait per answer is the
            // difference between keeping up and not. (Later, not here: a daemon that answers at
            // once must not be asked from inside its own answer. And not after an answer with
            // nothing in it, which asking again would not improve.)
            if (awaited && ok.rows.length > 0) Qt.callLater(_fill)
        })
    }

    /// Where the viewport will be by the time an answer to a request sent now gets back. Asking
    /// about where it is *now* is asking about the past: at a fling's speed the answer lands a
    /// thousand rows behind the screen, and those rows are drawn for nobody.
    function _predicted() {
        const n = count > 0 ? count : 1
        const lead = Math.round(_lastDirection * _velocity * Math.max(16, _rtt) / 1000)
        return Math.max(0, Math.min(n - 1, viewportFirst + lead))
    }

    // Ask for the whole wanted range whatever is held — what is held is not to be trusted.
    function _refetch() {
        const [first, end] = _wanted()
        _request(first, Math.min(Math.max(end - first, 1), maxRequest))
    }

    // One step of a `Splice`: every position after it moves by one.
    function _shift(op) {
        const out = {}
        for (const k in _rows) {
            const p = Number(k)
            if (op.op === "remove") { if (p < op.pos) out[p] = _rows[k]; else if (p > op.pos) out[p - 1] = _rows[k] }
            else out[p < op.pos ? p : p + 1] = _rows[k]
        }
        if (op.op === "insert" && op.row) out[op.pos] = op.row
        _rows = out
    }

    function _fill() {
        // What is held is known to be out of date and nothing is on its way to replace it.
        if (_stale && !_pending) { _refetch(); return }
        // An answer is on its way (and has not been for so long that it is never coming).
        if (_pending && Date.now() - _pendingAt < 2000) return
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
        // More missing than one request holds: the part nearest the viewport first.
        if (b - a > maxRequest && _lastDirection < 0) a = b - maxRequest
        // Falling behind a moving viewport. One answer holds `maxRequest` rows and a fling can
        // outrun that, so the question is not how to catch up — it is which rows to spend the
        // next answer on. Filling the gap that has opened behind buys nothing: those rows are
        // gone. Jump to where the screen will be instead, and leave the hole.
        // …but only when the screen is genuinely outrunning the answers. One answer carries
        // `maxRequest` rows; if the viewport travels less than that in a round trip, fetching
        // straight ahead keeps up and no gap ever opens, and jumping would only overshoot the
        // rows being looked at now. Columns, whose rows are cheap and whose answers come back in
        // half the time, is on the near side of that line; a list of formatted rows is not.
        const outrun = Math.abs(_predicted() - viewportFirst) > maxRequest
        if (_velocity > 0 && !visibleHeld && outrun) {
            const back = _lastDirection > 0 ? padBehind : Math.max(0, maxRequest - viewportCount - padBehind)
            const want = Math.max(0, Math.min(Math.max(count - 1, 0), _predicted() - back))
            // Past `end`, which is only as far as the look-ahead reaches: where the screen will be
            // is not a kind of look-ahead, and must not be clipped to it.
            if (want > a) { a = want; b = Math.min(count > 0 ? count : a + maxRequest, a + maxRequest) }
        }
        _request(a, Math.min(b - a, maxRequest))
    }

    function _apply(first, rows) {
        const t0 = Date.now()
        for (let i = 0; i < rows.length; i++) _rows[first + i] = rows[i]
        // Drop rows far outside the window so a long scroll does not keep everything.
        const far = Math.max(4 * padAhead, 2 * _ahead())
        const keepFrom = viewportFirst - Math.max(4 * padBehind, _lastDirection < 0 ? far : 0), keepTo = viewportFirst + viewportCount + far
        for (const k in _rows) { const p = Number(k); if (p < keepFrom || p > keepTo) delete _rows[k] }
        const t1 = Date.now()
        rowsUpdated(first, rows.length)
        _msStore += t1 - t0
        _msBind += Date.now() - t1
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
            _stale = true; count = msg.n; _reqFirst = -1; _reqEnd = -1
            _gen = msg.gen !== undefined ? msg.gen : -1
            for (const k in _rows) if (Number(k) >= count) delete _rows[k]
            reset()
            _refetch()
            break
        case "Splice": {
            // Already in the rows held: a `Window` answered after the change got here first.
            if (_gen >= 0 && msg.gen <= _gen) { count = msg.n; break }
            // A step was missed; start again from what the daemon has.
            if (_gen >= 0 && msg.gen > _gen + 1) { handleEvent({ event: "Reset", lid: msg.lid, n: msg.n, gen: msg.gen }); break }
            _gen = msg.gen
            let low = msg.n
            for (const op of msg.ops) { _shift(op); low = Math.min(low, op.pos) }
            count = msg.n; _reqFirst = -1; _reqEnd = -1
            spliced(msg.ops)
            // Every row from the first change on is at a new position: delegates read again.
            rowsUpdated(low, Math.max(1, count - low + 1))
            _fill()
            break
        }
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
