import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// Side by side (0.1.1): the accent marks the focused pane's selection. The other pane keeps its
// selection in the grey the columns view gives its trail, and it lights up again on focus.
TestCase {
    id: tc
    name: "SideBySideFocus"
    when: windowShown
    visible: true
    width: 1200; height: 760

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    Kiki.Shell { id: shell; width: 1200; height: 760; visible: true }

    function initTestCase() {
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.file("a.txt"), fake.file("b.txt")], "file:///home/u": [fake.file("x.txt")] }
        shell.left.listing.daemon = fake
        shell.right.listing.daemon = fake
    }
    function init() {
        shell.sideBySide = true
        shell.left.view = "list"; shell.right.view = "list"
        shell.left.open("file:///home/t"); shell.right.open("file:///home/u")
        wait(80)
        shell.focusPane(shell.left)
        shell.left.selection.set(0); shell.right.selection.set(0)
        wait(20)
    }
    function cleanup() { shell.sideBySide = false; wait(20) }
    function mark(side) { return findChild(findChild(findChild(shell, "pane-" + side), "row-0"), "rowmark") }

    // A location clicked while it is already open is a way back to its pane (owner, 2026-09-24:
    // "it should not connect again, just highlight the remote view"): nothing is re-opened, the
    // folders and selections stay, the right pane takes the focus.
    function test_clicking_an_open_location_only_focuses_its_pane() {
        const loc = { name: "u", localUri: "file:///home/t", remoteUri: "file:///home/u" }
        shell.right.selection.set(0); shell.left.selection.set(1)
        const lids = [shell.left.listing.lid, shell.right.listing.lid], hist = shell.right.history.length
        shell.openLocation(loc)
        wait(20)
        compare(shell.pane, shell.right, "the remote pane is focused")
        compare(shell.right.listing.lid, lids[1], "not opened again")
        compare(shell.left.listing.lid, lids[0])
        compare(shell.right.history.length, hist, "no new history entry")
        compare(shell.right.selection.current, 0, "the selection is kept")
        compare(shell.left.selection.current, 1)
        // Elsewhere in the location still counts as open; a pane outside it is brought there.
        shell.left.open("file:///home/u"); wait(40)
        shell.openLocation(loc); wait(40)
        compare(shell.left.uri, "file:///home/t", "the left pane was elsewhere: opened at the local folder")
        compare(shell.right.uri, "file:///home/u")
    }
    // Arriving at a server in one pane: that pane becomes the right pane with the listing it
    // just made, and the local folder opens beside it — two listings, not three (it used to
    // list the server again in the right pane and open the local folder over the first).
    function test_arriving_at_a_server_keeps_its_listing_and_opens_the_local_folder_beside_it() {
        fake.tree["sftp://lab/srv"] = [fake.file("s.txt")]
        shell.locations = [{ plugin: "sftp", name: "lab", localUri: "file:///home/t", remoteUri: "sftp://lab/srv" }]
        shell.sideBySide = false; wait(20)
        const opened = fake._nextLid                       // every Open takes a lid; closes give none back
        shell.left.open("sftp://lab/srv"); wait(80)
        verify(shell.sideBySide, "side by side on arrival")
        compare(shell.right.uri, "sftp://lab/srv", "the server on the right")
        compare(shell.left.uri, "file:///home/t", "the local folder on the left")
        compare(shell.pane, shell.right)
        compare(fake._nextLid - opened, 2, "one listing for the server, one for the local folder")
        shell.locations = []
    }
    function test_the_focused_panes_selection_is_the_accent_and_the_others_steps_back() {
        compare(mark("left").color, Kiki.Theme.accent, "the focused pane's row")
        compare(mark("right").color, Kiki.Theme.surface, "the other pane's row, kept but grey")
        shell.focusPane(shell.right)
        tryCompare(mark("right"), "color", Kiki.Theme.accent, 1000, "the focus moved: lit")
        tryCompare(mark("left"), "color", Kiki.Theme.surface, 1000, "and the left stepped back")
        shell.focusPane(shell.left)
        tryCompare(mark("left"), "color", Kiki.Theme.accent, 1000, "and back again")
    }
    function test_one_pane_is_always_the_accent() {
        shell.sideBySide = false; wait(40)
        compare(mark("left").color, Kiki.Theme.accent)
    }
}
