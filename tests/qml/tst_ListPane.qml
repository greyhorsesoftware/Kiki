import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// List view driven by the mouse against FakeDaemon (plan 28): this is the harness the rest of
// the interaction tests are built on — a real Pane, a real WindowCache, a scripted folder.
TestCase {
    id: tc
    name: "ListPane"
    when: windowShown
    visible: true
    width: 700; height: 400

    property var fake: null
    property var pane: null

    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        id: host
        anchors.fill: parent
        Views.ListPane { id: list; anchors.fill: parent; pane: tc.pane }
    }

    SignalSpy { id: activated; target: list; signalName: "activate" }
    SignalSpy { id: menued; target: list; signalName: "contextMenu" }
    SignalSpy { id: renamed; target: tc.pane; signalName: "renameRequested" }

    function init() {
        // Per-folder view memory and the socket log are singletons: a test that leaves either
        // set changes what the next one sees.
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.dir("Projects"), fake.file("a.txt", { size: 10 }), fake.file("b.txt", { size: 20 }), fake.file(".hidden")] }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        list.pane = pane
        renamed.target = pane
        pane.open("file:///home/t")
        wait(50)
        activated.clear(); menued.clear(); renamed.clear()
    }
    function cleanup() { list.pane = null; pane.destroy(); fake.destroy() }

    // The window cache asked for the folder and got its rows, hidden files left out.
    function test_listing_renders_rows() {
        compare(pane.listing.count, 3)
        compare(pane.listing.row(0).name, "Projects")   // folders first
        compare(pane.listing.row(1).name, "a.txt")
        verify(findChild(list, "row-1") !== null)
    }

    function test_click_selects_the_row_under_the_pointer() {
        const row = findChild(list, "row-1")
        verify(row !== null)
        mouseClick(row, 40, row.height / 2)
        compare(pane.selection.current, 1)
        verify(pane.selection.has(1))
    }

    function test_double_click_activates() {
        const row = findChild(list, "row-2")
        mouseDoubleClickSequence(row, 40, row.height / 2)
        compare(activated.count, 1)
        compare(activated.signalArguments[0][0], 2)
    }

    function test_right_click_asks_for_the_context_menu() {
        const row = findChild(list, "row-1")
        mouseClick(row, 40, row.height / 2, Qt.RightButton)
        compare(menued.count, 1)
        compare(menued.signalArguments[0][0], 1)
    }

    // Three rows in a 400px pane leaves empty space below them; a right click there is still a
    // menu, asked for with no row — the shell greys out everything that needs a file.
    function test_right_click_on_empty_space_asks_for_the_folder_menu() {
        pane.selection.set(1)
        mouseClick(list, 200, tc.height - 20, Qt.RightButton)
        compare(menued.count, 1)
        compare(menued.signalArguments[0][0], -1)
        compare(pane.selection.count(), 0)
    }

    // F2 puts the row into the inline editor; typing and Enter asks for the rename.
    function test_inline_rename_emits_the_request() {
        pane.selection.set(1)
        pane.renamingIndex = 1
        const editor = findChild(findChild(list, "row-1"), "renameEditor")
        verify(editor !== null)
        editor.forceActiveFocus()
        editor.text = ""
        keyClick(Qt.Key_N); keyClick(Qt.Key_O); keyClick(Qt.Key_W)
        keyClick(Qt.Key_Return)
        compare(renamed.count, 1)
        compare(renamed.signalArguments[0][0], "file:///home/t/a.txt")
        compare(renamed.signalArguments[0][1], "now")
        compare(pane.renamingIndex, -1)
    }

    // Sorting is a daemon round trip: the request goes out and the reset brings new rows back.
    function test_sort_by_size_descending_goes_through_the_daemon() {
        pane.setSort("size", "desc")
        wait(50)
        const req = fake.last("Sort")
        verify(req !== null)
        compare(req.fields.role, "size")
        compare(req.fields.order, "desc")
        compare(pane.listing.row(1).name, "b.txt")       // 20 bytes before 10
    }

    function test_hidden_files_appear_on_request() {
        pane.setHidden(true)
        wait(50)
        compare(fake.last("ShowHidden").fields.show, true)
        compare(pane.listing.count, 4)
    }
}
