import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

// Drives WindowCache with a fake daemon: no socket, replies are scripted.
TestCase {
    name: "WindowCache"
    when: windowShown
    visible: true

    property var sent: []
    property var fake: QtObject {
        property int _lid: 1
        property int total: 100000
        property string prefix: "f"
        // undefined: a daemon that does not number its answers (a tree, search results).
        property var gen: undefined
        // With `hold` set a Window's reply waits in `held` until the test delivers it.
        property bool hold: false
        property var held: []
        function allocLid() { return _lid++ }
        function bind(lid, l) { bound = l }
        function unbind(lid) { bound = null }
        property var bound: null
        function request(type, fields, cb) {
            sent.push({ type: type, fields: fields })
            if (type === "Open") cb({ cached: false }, undefined)
            if (type === "Window") {
                const rows = []
                for (let i = 0; i < fields.count && fields.first + i < total; i++) rows.push({ name: prefix + (fields.first + i), kind: "file", isDir: false, isLink: false, meta: null, thumb: null, git: null })
                const reply = { first: fields.first, rows: rows, n: total, done: true }
                if (gen !== undefined) reply.gen = gen
                if (hold) held.push(() => cb(reply, undefined)); else cb(reply, undefined)
            }
        }
    }

    Kiki.WindowCache { id: cache; daemon: fake; padAhead: 20; padBehind: 10; viewportCount: 10 }

    function init() { fake.hold = false; cache.debounce.stop(); cache.close(); cache._velocity = 0; cache._movedAt = 0; cache.viewportFirst = 0; cache.viewportCount = 10; cache._lastDirection = 1; sent = []; fake.total = 100000; fake.prefix = "f"; fake.gen = undefined; fake.hold = false; fake.held = [] }
    function names(a, b) { const out = []; for (let i = a; i < b; i++) { const r = cache.row(i); out.push(r ? r.name : null) } return out }

    function test_open_sends_open_and_first_window_together() {
        cache.open("file:///tmp")
        compare(sent[0].type, "Open")
        compare(sent[1].type, "Window")
        compare(sent[1].fields.first, 0)
        compare(cache.count, 100000)
        verify(cache.row(0) !== null)
        compare(cache.row(0).name, "f0")
        verify(cache.row(5000) === null)
    }

    function test_viewport_requests_only_missing_rows() {
        cache.open("file:///tmp")
        sent = []
        cache.setViewport(5000, 10)
        wait(40)  // debounce
        compare(sent.length, 1)
        compare(sent[0].type, "Window")
        compare(sent[0].fields.first, 4990)
        compare(sent[0].fields.count, 40)
        compare(cache.row(5000).name, "f5000")
        sent = []
        cache.setViewport(5002, 10)
        wait(40)
        compare(sent.length, 0)  // already held
    }

    function test_reset_clears_and_refetches() {
        cache.open("file:///tmp")
        sent = []
        fake.total = 50
        cache.handleEvent({ event: "Reset", lid: cache.lid, n: 50 })
        compare(cache.count, 50)
        verify(cache.row(0) !== null)   // refetched after the clear
        compare(sent.length, 1)
        compare(sent[0].type, "Window")
    }

    function test_rows_event_patches_in_place() {
        cache.open("file:///tmp")
        cache.handleEvent({ event: "Rows", lid: cache.lid, first: 2, rows: [{ name: "patched", kind: "file", isDir: false, isLink: false, meta: { size: 7 }, thumb: null, git: null }] })
        compare(cache.row(2).name, "patched")
        compare(cache.row(2).meta.size, 7)
    }

    // A Reset used to empty the cache at once, so every row and thumbnail on screen went blank
    // for a round trip — and nearly every Reset (a rescan) brings the same rows straight back.
    function test_reset_keeps_rows_on_screen_until_the_new_ones_arrive() {
        cache.open("file:///tmp")
        fake.hold = true; fake.prefix = "g"; fake.total = 50
        cache.handleEvent({ event: "Reset", lid: cache.lid, n: 50 })
        compare(cache.count, 50)
        compare(cache.row(3).name, "f3")          // still what was there
        verify(cache.row(50) === null)            // but nothing past the new end
        compare(fake.held.length, 1)              // asked for again although "held"
        fake.held.shift()()
        compare(cache.row(3).name, "g3")
        compare(names(28, 31), ["g28", "g29", null])
    }

    function test_nothing_stale_survives_the_new_rows() {
        cache.open("file:///tmp")
        cache.setViewport(200, 10); wait(40)
        verify(cache.row(205) !== null)
        cache.setViewport(0, 10); wait(40)
        fake.prefix = "g"
        cache.handleEvent({ event: "Reset", lid: cache.lid, n: 100000 })
        verify(cache.row(205) === null)           // outside what came back: gone, not left stale
        compare(cache.row(0).name, "g0")
    }

    function test_splice_moves_rows_without_asking_again() {
        fake.gen = 4
        cache.open("file:///tmp")
        sent = []
        let told = null, updated = null
        const onS = ops => told = ops, onU = (first, n) => updated = [first, n]
        cache.spliced.connect(onS); cache.rowsUpdated.connect(onU)
        cache.handleEvent({ event: "Splice", lid: cache.lid, n: 100000, gen: 5, ops: [
            { op: "remove", pos: 1 },
            { op: "insert", pos: 3, row: { name: "new", kind: "file", isDir: false, isLink: false, meta: null, thumb: null, git: null } } ] })
        cache.spliced.disconnect(onS); cache.rowsUpdated.disconnect(onU)
        compare(names(0, 6), ["f0", "f2", "f3", "new", "f4", "f5"])
        compare(told.length, 2)
        compare(updated[0], 1)
        compare(sent.length, 0)                   // nothing thrown away, nothing fetched
    }

    function test_a_removal_leaves_a_hole_at_the_far_end_that_is_filled() {
        fake.gen = 1
        cache.open("file:///tmp")                 // holds 0..29
        fake.gen = 2; fake.total = 99999
        sent = []
        cache.handleEvent({ event: "Splice", lid: cache.lid, n: 99999, gen: 2, ops: [{ op: "remove", pos: 0 }] })
        compare(cache.count, 99999)
        compare(cache.row(0).name, "f1")
        wait(40)
        verify(cache.row(29) !== null || sent.length <= 1)
    }

    // The reply and the event travel separately. A Window answered after the change can arrive
    // before the Splice that announces it: applying the Splice then would move rows twice.
    function test_a_splice_already_in_the_rows_is_not_applied_twice() {
        fake.gen = 7
        cache.open("file:///tmp")
        cache.handleEvent({ event: "Splice", lid: cache.lid, n: 99999, gen: 7, ops: [{ op: "remove", pos: 0 }] })
        compare(cache.row(0).name, "f0")
        compare(cache.count, 99999)
    }

    // And one answered before the change can arrive after word of it: its positions are old.
    function test_a_reply_older_than_the_rows_is_dropped_and_asked_again() {
        fake.gen = 3
        cache.open("file:///tmp")
        fake.hold = true
        cache.setViewport(1000, 10); wait(40)
        compare(fake.held.length, 1)              // computed at gen 3, still in flight
        fake.gen = 4; fake.prefix = "h"
        cache.handleEvent({ event: "Splice", lid: cache.lid, n: 100001, gen: 4, ops: [{ op: "insert", pos: 0, row: { name: "first", kind: "file", isDir: false, isLink: false, meta: null, thumb: null, git: null } }] })
        compare(fake.held.length, 1)              // one question at a time: nothing new asked yet
        fake.held.shift()()                       // the old answer lands…
        verify(cache.row(1000) === null)          // …and is not shown
        wait(40)
        compare(fake.held.length, 1)              // but asked again
        compare(cache.row(0).name, "first")
        fake.held.shift()()
        compare(cache.row(1000).name, "h1000")
    }

    function test_a_missed_step_starts_again() {
        fake.gen = 1
        cache.open("file:///tmp")
        fake.gen = 5; fake.prefix = "z"
        sent = []
        cache.handleEvent({ event: "Splice", lid: cache.lid, n: 100000, gen: 5, ops: [{ op: "remove", pos: 0 }] })
        compare(sent.length, 1)
        compare(cache.row(0).name, "z0")
    }

    // A scroll moves the viewport every frame. The timer used to be restarted on each move, so
    // while the scrolling went on it never fired: no rows were asked for until it stopped.
    function test_a_steady_scroll_is_fed_while_it_is_still_moving() {
        cache.open("file:///tmp")
        sent = []
        for (let i = 1; i <= 30; i++) { cache.setViewport(i * 40, 10); wait(8) }   // a move every 8ms
        verify(sent.length >= 3, "asked " + sent.length + " times during the scroll")
        verify(cache.row(30 * 40 - 40) !== null || cache.row(30 * 40) !== null)
    }

    // One question at a time: the next is asked when the answer lands, for wherever the view is
    // by then — not one per frame, piling up behind a daemon that is still answering the first.
    function test_only_one_window_is_in_flight() {
        cache.open("file:///tmp")
        fake.hold = true
        sent = []
        for (let i = 1; i <= 10; i++) { cache.setViewport(i * 500, 10); wait(20) }
        compare(sent.length, 1)
        fake.held.shift()()                        // it lands…
        wait(40)
        compare(sent.length, 2)                    // …and only then is the next asked,
        // …for where the view is going, never for where it has been.
        verify(sent[1].fields.first >= 5000 - cache.padBehind, "asked at " + sent[1].fields.first + ", viewport at 5000")
        compare(fake.held.length, 1)
        fake.hold = false
        fake.held.shift()()
        // While it is still moving it is asking about where the screen is going, so the row under
        // the pointer *now* may not be held yet. Once it stops, that is what gets filled.
        tryVerify(() => cache.row(5000) !== null, 3000, "the viewport fills once the scroll stops")
    }

    function test_an_empty_folder_is_asked_once() {
        fake.total = 0
        cache.open("file:///tmp")
        wait(60)
        compare(sent.filter(r => r.type === "Window").length, 1)
    }

    // A daemon that only ever answers from the past must not be asked for ever.
    function test_a_daemon_stuck_in_the_past_is_believed_in_the_end() {
        fake.gen = 4
        cache.open("file:///tmp")
        sent = []
        cache.handleEvent({ event: "Splice", lid: cache.lid, n: 99999, gen: 5, ops: [{ op: "remove", pos: 1 }] })
        cache.setViewport(3000, 10)
        wait(400)
        verify(sent.length <= 8, "asked " + sent.length + " times")
        verify(cache.row(3000) !== null)
    }

    // One answer holds 512 rows and a fling can outrun that, so the question is not how to catch
    // up — it is which rows to spend the next answer on. The gap that has opened behind the
    // viewport is rows nobody will look at again; where the screen will be when the answer lands
    // is the only place worth asking about.
    function test_a_fling_asks_where_the_screen_will_be_not_where_it_was() {
        cache.open("file:///tmp")
        fake.hold = true
        // 500 rows every 20ms — 25,000 a second, which is what a flung list does.
        for (let i = 1; i <= 6; i++) { cache.setViewport(i * 500, 10); wait(20) }
        fake.held.shift()()                        // the one in flight lands
        wait(60)
        const f = sent[sent.length - 1].fields
        const going = cache._predicted()
        verify(f.first >= 3000 - cache.padBehind, "asked at " + f.first + " with the screen at 3000: back in the hole")
        verify(going > 3000, "the prediction should be ahead of the screen, not " + going)
        verify(f.first <= going && f.first + f.count > going, "asked " + f.first + "+" + f.count + ", which does not cover " + going)
        fake.hold = false
    }

    // Stopped, there is nothing to predict: the hole behind is exactly what wants filling.
    function test_a_still_view_still_fills_what_is_missing() {
        cache.open("file:///tmp")
        cache.setViewport(9000, 10)
        wait(60)
        verify(cache.row(9000) !== null)
        verify(cache.row(9005) !== null)
    }
}
