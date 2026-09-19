import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// `Pane.rememberViews`: the one switch that turns per-folder view memory off, for reading and
// for writing, which is what side by side needs. And "mirror", which used to be a view.
TestCase {
    id: tc
    name: "PaneViewMemory"

    property var fake: null
    property var pane: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    function init() {
        Wire.reset(); Wire.connectAll()
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { "default": "list", rememberPerFolder: true })
        Kiki.Settings.viewPrefs = ({
            "file:///home/t/photos": { view: "gallery", sort: "mtime", order: "desc", hidden: false },
            "file:///home/t/old": { view: "mirror", sort: "size", order: "asc", hidden: false },
        })
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [], "file:///home/t/photos": [], "file:///home/t/old": [], "file:///home/t/plain": [] }
        pane = paneC.createObject(tc); pane.listing.daemon = fake
        Wire.reset()
    }
    function cleanup() { pane.destroy(); fake.destroy() }

    function test_remembering_a_folder_opens_the_way_it_was_left() {
        pane.open("file:///home/t/photos")
        compare(pane.view, "gallery")
        compare(pane.sortRole, "mtime")
    }

    function test_not_remembering_nothing_is_read() {
        pane.rememberViews = false
        pane.view = "list"
        pane.open("file:///home/t/photos")
        compare(pane.view, "list")                 // not the remembered Gallery
        compare(pane.sortRole, "name")             // nor its sort
        compare(pane.hasPref, false)
    }

    function test_not_remembering_nothing_is_written() {
        pane.rememberViews = false
        pane.open("file:///home/t/plain")
        pane.view = "icon"
        pane.setSort("size", "desc")
        pane.setHidden(true)
        compare(Wire.count("SetViewPref"), 0)
        verify(Kiki.Settings.viewPrefs["file:///home/t/plain"] === undefined)
    }

    function test_remembering_again_a_change_is_written() {
        pane.open("file:///home/t/plain")
        pane.view = "icon"
        compare(Wire.count("SetViewPref"), 1)
        compare(Wire.last("SetViewPref").view, "icon")
    }

    // The pane that stays when side by side is turned off: split it was a List, and now it goes
    // back to what the folder remembers.
    function test_applyPref_puts_the_folder_back_the_way_it_is_remembered() {
        pane.rememberViews = false
        pane.view = "list"
        pane.open("file:///home/t/photos")
        compare(pane.view, "list")
        pane.rememberViews = true
        pane.applyPref()
        compare(pane.view, "gallery")
        compare(pane.sortRole, "mtime")
        compare(Wire.count("SetViewPref"), 0)       // restoring is not a choice to remember
    }

    // An old views.toml entry from when the two-pane layout was a view. Nobody chose it.
    function test_a_stored_mirror_view_is_read_as_no_view() {
        pane.open("file:///home/t/old")
        compare(pane.view, "list")
        compare(pane.sortRole, "size")             // the rest of the entry still counts
    }

    function test_a_default_of_mirror_means_list_for_a_pane() {
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { "default": "mirror" })
        pane.open("file:///home/t/plain")
        compare(pane.view, "list")
    }
}
