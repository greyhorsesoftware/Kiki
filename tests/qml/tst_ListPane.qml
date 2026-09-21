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
    /// How wide the pane is. A property so a test can narrow it, as side by side and the
    /// inspector do, and watch the columns re-clamp.
    property int paneWidth: tc.width

    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        id: host
        anchors.fill: parent
        Views.ListPane { id: list; width: tc.paneWidth; height: parent.height; pane: tc.pane }
    }

    SignalSpy { id: activated; target: list; signalName: "activate" }
    SignalSpy { id: menued; target: list; signalName: "contextMenu" }
    SignalSpy { id: renamed; target: tc.pane; signalName: "renameRequested" }

    function init() {
        // Per-folder view memory, the column widths and the socket log are singletons: a test
        // that leaves any of them set changes what the next one sees.
        Kiki.Settings.viewPrefs = ({})
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { listColumnWidths: ({}) })
        tc.paneWidth = tc.width
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

    // ---------------------------------------------------------------- dragging a row out

    // Plan 29 assumed a drag could be started with the right button and nobody had seen one. It
    // can, on Qt's side of the glass: the same press and move starts a drag with either button,
    // carrying the same files. Whether the COMPOSITOR carries a right-button drag is the other
    // half, and stays on the manual list — nothing headless here can hold a button down.
    Component { id: spyC; SignalSpy { signalName: "dragStarted" } }
    function pull(item, x, y, dx, dy, button) {
        mousePress(item, x, y, button)
        for (let k = 1; k <= 6; k++) mouseMove(item, x + dx * k / 6, y + dy * k / 6, -1, button)
        wait(50)                                   // an automatic drag starts from a queued event
        mouseRelease(item, x + dx, y + dy, button)
    }
    function dragProxyOf(row) { return row.children.find(c => c.Drag !== undefined && c.width === 0) }

    function test_either_button_starts_a_drag() {
        const row = findChild(list, "row-1")                       // a.txt
        const proxy = dragProxyOf(row)
        verify(proxy !== undefined, "the row has a drag proxy")
        const spy = spyC.createObject(tc, { target: proxy.Drag })

        pull(row, row.width / 2, row.height / 2, 40, 60, Qt.LeftButton)
        compare(spy.count, 1, "the left button drags")
        compare(proxy.Drag.mimeData["text/uri-list"], "file:///home/t/a.txt\r\n")

        spy.clear()
        pull(row, row.width / 2, row.height / 2, 40, 60, Qt.RightButton)
        compare(spy.count, 1, "so does the right button")
        compare(proxy.Drag.mimeData["text/uri-list"], "file:///home/t/a.txt\r\n")
        spy.destroy()
    }

    // ---------------------------------------------------------------- resizing a column
    //
    // The value columns are laid out from the right edge with Name taking the rest, so the edge
    // that can follow the pointer is the one a column BEGINS at: drag it and that column widens
    // or narrows while Name absorbs the difference. At 700px wide the columns start at their
    // defaults — Modified 160, Size 80, Kind 120 — which leaves Name 280.

    function grip(role) { const g = findChild(list, "col-grip-" + role); verify(g !== null, role + " has a grip"); return g }
    function headerWidth(role) { return findChild(list, "header-" + role).width }
    /// Where the rightmost header cell ends, in the pane's own coordinates: the columns fit when
    /// this is the pane's width less its margin, and never more.
    function headerRight() {
        const c = list.columns[list.columns.length - 1]
        const h = findChild(list, "header-" + c.role)
        return h.mapToItem(list, h.width, 0).x
    }
    /// A frame, so the header Row puts its cells where the new widths say: a width is set the
    /// moment it is assigned, but nothing has MOVED until the layout has run. Drawing the pane
    /// runs it there and then — a plain `wait` only hopes the render loop gets a turn.
    function settle() { grabImage(list) }
    /// Press a grip, walk the pointer `dx` and let go. The grip travels with the edge it is on,
    /// so each step is aimed in the pane's coordinates and mapped back into the grip where it now
    /// is — which is what the pointer does of its own accord.
    function dragGrip(role, dx, steps) {
        const g = grip(role)
        const from = g.mapToItem(list, g.width / 2, g.height / 2)
        const n = steps || 4
        mousePress(g, g.width / 2, g.height / 2)
        let p = null
        for (let k = 1; k <= n; k++) {
            p = list.mapToItem(g, from.x + dx * k / n, from.y)
            mouseMove(g, p.x, p.y, -1, Qt.LeftButton)
            settle()
        }
        mouseRelease(g, p.x, p.y)
    }

    // Dragging the edge Modified begins at to the left widens Modified; Name gives up exactly what
    // it takes, and the cells in the rows are the same Items, rebound — not built again.
    function test_dragging_a_column_edge_resizes_it_and_the_rows_follow() {
        const cell = findChild(findChild(list, "row-1"), "cell-mtime")
        verify(cell !== null)
        compare(cell.width, 160)
        const name = list.nameWidth
        dragGrip("mtime", -40)
        compare(headerWidth("mtime"), 200)
        compare(cell.width, 200, "the row's cell followed the header")
        compare(list.nameWidth, name - 40, "and Name absorbed the difference")
    }

    // A column is never squeezed past its own header: the label and the sort arrow have to fit.
    function test_a_column_never_goes_below_its_header() {
        const min = list.columnMin(list.allColumns.size)
        verify(min > 40 && min < 80, min)
        dragGrip("size", 400)
        compare(headerWidth("size"), min)
        compare(list.nameWidth, 700 - 24 - (160 + 12) - (min + 12) - (120 + 12))
    }

    // Drag one column as wide as it will go and the others keep their widths: what is left when
    // they and a readable Name have had their share is all it may take. Nothing is drawn outside
    // the pane at any point.
    function test_a_column_cannot_push_the_others_off_the_pane() {
        dragGrip("mtime", -1000)
        compare(headerWidth("size"), 80, "Size is where it was")
        compare(headerWidth("kind"), 120, "and so is Kind")
        compare(list.nameWidth, list.minNameWidth, "Name is down to its minimum")
        compare(headerWidth("mtime"), 280)
        settle()
        compare(headerRight(), list.width - 12, "the last column ends at the pane's margin")
    }

    // Side by side, or the inspector opening, narrows the pane under the columns. They squeeze
    // towards their own minimums first and only drop off the right when even those do not fit —
    // and the squeeze is the pane's doing, so the remembered widths come back with the room.
    function test_narrowing_the_pane_re_clamps_the_columns() {
        list.setColumnWidth("mtime", 260); list.endColumnResize()
        compare(headerWidth("mtime"), 260)
        compare(list.columns.length, 4)

        tc.paneWidth = 420
        compare(list.columns.length, 3, "Kind has no room left")
        verify(headerWidth("mtime") < 260, headerWidth("mtime"))
        compare(list.nameWidth, list.minNameWidth)
        settle()
        compare(headerRight(), 420 - 12, "still nothing outside the pane")

        tc.paneWidth = 60
        compare(list.columns.length, 1, "nothing but Name is left")
        compare(list.nameWidth, 48, "and Name keeps its own floor")

        tc.paneWidth = tc.width
        compare(list.columns.length, 4)
        compare(headerWidth("mtime"), 260, "the squeeze was never written down")
    }

    // Double-click an edge and that column goes back to the width it ships with.
    function test_double_clicking_a_grip_puts_the_column_back() {
        dragGrip("mtime", -40)
        compare(headerWidth("mtime"), 200)
        Wire.reset()
        const g = grip("mtime")
        mouseDoubleClickSequence(g, g.width / 2, g.height / 2)
        compare(headerWidth("mtime"), 160)
        compare(Wire.count("SetSettings"), 1)
        // `null`, not merely absent: the daemon merges maps, and a width left out of one is a width kept.
        verify(Wire.last("SetSettings").patch.view.listColumnWidths.mtime === null, "the daemon is told to forget the width")
    }

    // The grip lies over the header cell and keeps the press, so a drag on the edge is never also
    // a click on the cell. A click on the cell itself still sorts.
    function test_a_drag_on_the_grip_does_not_sort_and_a_click_still_does() {
        const was = pane.sortRole, sorts = fake.count("Sort")
        dragGrip("mtime", 30)                       // to the right: the release stays over the cell
        compare(pane.sortRole, was, "the drag did not sort")
        compare(fake.count("Sort"), sorts, "and asked the daemon for nothing")
        compare(headerWidth("mtime"), 130, "it resized instead")

        const h = findChild(list, "header-mtime")
        mouseClick(h, h.width / 2, h.height / 2)
        wait(50)
        compare(pane.sortRole, "mtime")
        compare(fake.count("Sort"), sorts + 1)
    }

    // The affordance is the pointer and nothing else: the system's horizontal-resize cursor over
    // the edge, no line drawn, and no grip on Name — it is the column that absorbs.
    function test_the_edge_asks_for_the_resize_cursor() {
        const g = grip("mtime")
        compare(g.cursorShape, Qt.SplitHCursor)
        verify(g.hoverEnabled)
        compare(grip("name").visible, false)
    }

    // settings.toml is written once, when the button comes up — not per pixel — and as a patch of
    // the one key.
    function test_the_width_is_written_once_when_the_drag_ends() {
        Wire.reset()
        const g = grip("mtime")
        const from = g.mapToItem(list, g.width / 2, g.height / 2)
        mousePress(g, g.width / 2, g.height / 2)
        let p = list.mapToItem(g, from.x - 20, from.y)
        mouseMove(g, p.x, p.y, -1, Qt.LeftButton)
        compare(headerWidth("mtime"), 180, "the header follows the pointer at once")
        compare(Wire.count("SetSettings"), 0, "and nothing is written while it moves")
        settle()
        p = list.mapToItem(g, from.x - 40, from.y)
        mouseMove(g, p.x, p.y, -1, Qt.LeftButton)
        compare(Wire.count("SetSettings"), 0)
        mouseRelease(g, p.x, p.y)

        compare(Wire.count("SetSettings"), 1)
        const patch = Wire.last("SetSettings").patch
        compare(Object.keys(patch).join(","), "view")
        compare(Object.keys(patch.view).join(","), "listColumnWidths")
        compare(patch.view.listColumnWidths.mtime, 200)
    }

    // The widths are global to list view and come back from settings.toml; a width for a column
    // that is switched off is kept rather than thrown away by the next resize.
    function test_widths_come_back_from_settings_and_a_hidden_column_keeps_its_own() {
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { listColumnWidths: { mtime: 240, atime: 130 } })
        compare(headerWidth("mtime"), 240)
        compare(list.nameWidth, 700 - 24 - (240 + 12) - (80 + 12) - (120 + 12))
        verify(findChild(list, "header-atime") === null, "Accessed is switched off")

        dragGrip("size", 20)
        const w = Wire.last("SetSettings").patch.view.listColumnWidths
        compare(w.mtime, 240)
        compare(w.size, 60)
        compare(w.atime, 130, "the switched-off column keeps its width")
    }
}
