import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

// Drives WindowCache with a fake daemon: no socket, replies are scripted.
TestCase {
    name: "WindowCache"

    property var sent: []
    property var fake: QtObject {
        property int _lid: 1
        property int total: 100000
        function allocLid() { return _lid++ }
        function bind(lid, l) { bound = l }
        function unbind(lid) { bound = null }
        property var bound: null
        function request(type, fields, cb) {
            sent.push({ type: type, fields: fields })
            if (type === "Open") cb({ cached: false }, undefined)
            if (type === "Window") {
                const rows = []
                for (let i = 0; i < fields.count; i++) rows.push({ name: "f" + (fields.first + i), kind: "file", isDir: false, isLink: false, meta: null, thumb: null, git: null })
                cb({ first: fields.first, rows: rows, n: total, done: true }, undefined)
            }
        }
    }

    Kiki.WindowCache { id: cache; daemon: fake; padAhead: 20; padBehind: 10; viewportCount: 10 }

    function init() { sent = []; fake.total = 100000 }

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
}
