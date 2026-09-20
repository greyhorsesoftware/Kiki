import QtQuick

// Scrolls a view from top to bottom at a steady pace and says what that cost: how long each
// frame took, how often a row that should have been on screen was not there yet, and how long
// the view took to fill in once the scrolling stopped. The scroll_perf flow reads this through
// the shell's IPC; nothing in the window depends on it.
Item {
    id: probe
    visible: false

    property Item target: null          // the Flickable being driven
    property var cache: null            // the WindowCache behind it
    property bool running: false
    property var result: ({})

    property real _to: 0
    property int _ms: 0
    property double _t0: 0
    property double _last: 0
    property double _stopped: 0
    property var _frames: []
    property int _blankFrames: 0
    property int _blankRows: 0
    property int _serial0: 0

    /// Top to bottom in `ms`. The pace is the caller's: 100,000 rows in four seconds is a fling
    /// no hand makes, and that is the point — nothing a person does can be worse.
    function start(flickable, windowCache, ms) {
        if (!flickable || !windowCache) { result = { error: "no view to scroll" }; return }
        target = flickable; cache = windowCache; _ms = Math.max(1, ms)
        target.contentY = target.originY
        _to = Math.max(0, target.contentHeight - target.height)
        _frames = []; _blankFrames = 0; _blankRows = 0; _stopped = 0
        _serial0 = cache._serial
        cache.resetCost()
        _t0 = Date.now(); _last = _t0
        result = { running: true }
        running = true
    }

    function _missing() {
        let n = 0
        const end = Math.min(cache.count, cache.viewportFirst + cache.viewportCount)
        for (let i = cache.viewportFirst; i < end; i++) if (!cache.row(i)) n++
        return n
    }

    FrameAnimation {
        running: probe.running
        onTriggered: {
            const now = Date.now()
            if (probe._stopped === 0) {
                probe._frames.push(now - probe._last); probe._last = now
                const missing = probe._missing()
                if (missing > 0) { probe._blankFrames++; probe._blankRows += missing }
                const f = Math.min(1, (now - probe._t0) / probe._ms)
                probe.target.contentY = probe.target.originY + probe._to * f
                if (f >= 1) probe._stopped = now
                return
            }
            // Stopped at the bottom: how long until every row in view is there.
            if (probe._missing() > 0 && now - probe._stopped < 10000) return
            const fr = probe._frames.slice(1).sort((a, b) => a - b)     // the first is the wait to begin
            const sum = fr.reduce((a, b) => a + b, 0)
            probe.running = false
            probe.result = {
                running: false, rows: probe.cache.count, ms: probe._ms,
                frames: fr.length,
                avgMs: fr.length ? Math.round(sum / fr.length * 10) / 10 : 0,
                p95Ms: fr.length ? fr[Math.floor(fr.length * 0.95)] : 0,
                worstMs: fr.length ? fr[fr.length - 1] : 0,
                over33: fr.filter(x => x > 33).length,
                blankFrames: probe._blankFrames, blankRows: probe._blankRows,
                settleMs: now - probe._stopped,
                requests: probe.cache._serial - probe._serial0,
                // Where a round trip goes, per answer: waiting for it, storing it, and the
                // delegates re-reading their rows. What a lean row could save is the wait.
                answers: probe.cache._answers,
                waitMs: probe.cache._answers ? Math.round(probe.cache._msWait / probe.cache._answers * 10) / 10 : 0,
                storeMs: probe.cache._answers ? Math.round(probe.cache._msStore / probe.cache._answers * 10) / 10 : 0,
                bindMs: probe.cache._answers ? Math.round(probe.cache._msBind / probe.cache._answers * 10) / 10 : 0,
                lastRow: probe.cache.viewportFirst + probe.cache.viewportCount
            }
        }
    }
}
