import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The chooser kiki gives other apps through the portal, asked by kiki itself: the answer comes
// back to the caller and nothing is sent to the daemon as a portal result.
TestCase {
    id: tc
    name: "PortalPick"
    when: windowShown
    visible: true
    width: 900; height: 600

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    UI.PortalDialog { id: portal; anchors.fill: parent; home: "/home/t" }

    function init() {
        Wire.reset(); Wire.connectAll()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.dir("Sites")], "file:///home/t/Sites": [] }
        portal.pane.listing.daemon = fake
    }
    function cleanup() { portal.visible = false; portal._local = null; fake.destroy() }

    function test_choosing_a_folder_answers_the_caller_and_not_the_daemon() {
        let got = "unset"
        portal.pick({ mode: "open", directory: true, title: "Choose the local folder", currentFolder: "/home/t/Sites" }, uris => got = uris)
        verify(portal.visible)
        compare(portal.pane.uri, "file:///home/t/Sites")
        Wire.reset()
        portal.accept()
        verify(!portal.visible)
        compare(got.length, 1)
        compare(got[0], "file:///home/t/Sites")
        compare(Wire.count("ChooserResult"), 0)
    }

    // A chooser in `open` mode takes the keyboard when it appears: the window's own keys stand
    // down while it is up, so a dialog that did not would leave Escape with nobody to hear it.
    function test_an_open_chooser_takes_focus_so_escape_reaches_it() {
        let got = "unset"
        portal.pick({ mode: "open", directory: false, currentFolder: "/home/t" }, uris => got = uris)
        verify(portal.activeFocus, "the dialog has the keyboard")
        keyClick(Qt.Key_Escape)
        compare(got, null, "and Escape cancels it")
        verify(!portal.visible)
    }

    function test_cancelling_answers_null() {
        let got = "unset"
        portal.pick({ mode: "open", directory: true, currentFolder: "/home/t" }, uris => got = uris)
        portal.finish(null)
        compare(got, null)
        compare(Wire.count("ChooserResult"), 0)
    }

    // A portal request after a local pick is answered by token to whoever asked — since 0.3.0
    // that is the window itself, which `kiki-dbus` collects from; nothing goes to the daemon.
    // The callback is one-shot, so the local pick before it must not swallow this one.
    function test_a_portal_request_after_a_local_pick_answers_by_token() {
        portal.pick({ mode: "open", directory: true, currentFolder: "/home/t" }, () => {})
        portal.finish(null)
        Wire.reset()
        let answered = null
        portal.chooser = ({ answered: (token, uris) => answered = { token: token, uris: uris } })
        portal.open({ mode: "open", directory: true, currentFolder: "/home/t", token: "tok-1" })
        portal.accept()
        verify(answered !== null, "the window was told")
        compare(answered.token, "tok-1")
        compare(Wire.count("ChooserResult"), 0, "and the daemon was not")
    }

    // The side part is the main window's rail, with favorites and nothing else: no search (a
    // chooser has nowhere for one to go) and no locations — one can only answer with a URI the
    // asking application cannot open.
    function test_the_side_part_is_the_rail_with_favorites_only() {
        const rail = findChild(tc, "chooser-rail")
        verify(rail !== null)
        verify(rail.compact, "the rail, as the main window's default")
        compare(rail.width, 44)
        // It follows the person's setting, and does nothing of its own: traditional in the app
        // is traditional here, with no hover to widen it.
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { sidebarStyle: "traditional" })
        verify(!rail.compact, "traditional, as set")
        verify(rail.width > 44)
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { sidebarStyle: "rail" })
        verify(rail.compact)
        verify(rail.showSearch, "the rail's Search entry is offered")
        verify(!rail.showLocations, "no Locations section at all — not Trash, not a server, not 'add'")
        verify(rail.entries.every(e => e.kind === "favorite" || e.kind === "search"), JSON.stringify(rail.entries.map(e => e.kind)))
        // The rail's Search entry is search everywhere — the main window's overlay, not a box.
        portal.pick({ mode: "open", directory: false, currentFolder: "/home/t" }, () => {})
        const all = findChild(tc, "chooser-search-all")
        verify(all !== null)
        verify(!all.visible, "down until asked for")
        rail.searchRequested()
        verify(all.visible, "the rail's entry brings the overlay up")
        verify(rail.searchOpen, "and the entry shows it is up")
        rail.searchRequested()
        verify(!all.visible)
        portal.finish(null)
    }

    // The main window's keys that mean something here: a dot for hidden files, a slash for the
    // filter box. Without them a chooser had Escape and nothing else.
    function test_dot_and_slash_work_in_the_chooser() {
        portal.pick({ mode: "open", directory: false, currentFolder: "/home/t" }, () => {})
        const box = findChild(tc, "chooser-filter")
        const was = portal.pane.showHidden
        keyClick(Qt.Key_Period)
        compare(portal.pane.showHidden, !was, "a dot toggles hidden files")
        keyClick(Qt.Key_Period)
        compare(portal.pane.showHidden, was)
        verify(!box.visible)
        keyClick(Qt.Key_Slash)
        verify(box.visible, "a slash brings the filter bar up — the main window's, across the list")
        box.closed()
        verify(!box.visible, "and its Escape takes it down")
        portal.finish(null)
    }

    // The box fits the window it is in. It was 860 × 560 whatever the window, and in one shorter
    // than that (the owner's, 625 px with the title bar) its buttons were below the edge.
    function buttons() {
        const out = []
        function walk(it) { for (const c of it.children) { if (c.text !== undefined && c.clicked !== undefined && c.primary !== undefined) out.push(c); walk(c) } }
        walk(portal); return out.filter(b => b.visible)
    }
    function test_it_fits_a_short_window_data() { return [{ tag: "roomy", w: 1200, h: 760 }, { tag: "the owner's", w: 1176, h: 590 }, { tag: "small", w: 700, h: 420 }] }
    function test_it_fits_a_short_window(data) {
        portal.anchors.fill = undefined; portal.width = data.w; portal.height = data.h
        portal.pick({ mode: "save", title: "Save the mirror report", currentFolder: "/home/t", currentName: "kiki-mirror-ghs.txt" }, () => {})
        wait(20)
        const labels = buttons().map(b => b.text)
        verify(labels.indexOf("Save") >= 0 && labels.indexOf("Cancel") >= 0, labels.join(","))
        for (const b of buttons()) {
            const p = b.mapToItem(portal, 0, 0)
            verify(p.y >= 0 && p.y + b.height <= portal.height, b.text + " at y " + p.y + "–" + (p.y + b.height) + " in " + portal.height)
            verify(p.x >= 0 && p.x + b.width <= portal.width, b.text + " at x " + p.x + "–" + (p.x + b.width) + " in " + portal.width)
        }
        portal.finish(null)
        portal.anchors.fill = tc
    }
}
