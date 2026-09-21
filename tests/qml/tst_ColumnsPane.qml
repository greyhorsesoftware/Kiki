import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// Miller columns under the mouse. `columns` is replaced on every selection; the view must not
// answer that by rebuilding itself, or every click reloads every icon in every column (a flash).
TestCase {
    id: tc
    name: "ColumnsPane"
    when: windowShown
    visible: true
    width: 900; height: 400

    property var fake: null
    property var pane: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        anchors.fill: parent
        Views.ColumnsPane { id: cols; anchors.fill: parent; pane: tc.pane }
    }

    function init() {
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.dir("Projects"), fake.file("a.txt"), fake.file("b.txt")] }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        cols.pane = pane
        pane.open("file:///home/t")
        cols.rebuild()
        wait(50)
    }
    // The columns hold the pane's own listing; let go of it before the pane goes.
    function cleanup() { cols.columns = []; cols.pane = null; pane.destroy(); fake.destroy() }

    function test_a_click_keeps_every_delegate_it_did_not_have_to_touch() {
        const column = findChild(cols, "column-0")
        const clicked = findChild(cols, "colrow-0-1")
        const bystander = findChild(cols, "colrow-0-2")
        verify(column !== null && clicked !== null && bystander !== null)

        mouseClick(clicked, clicked.width / 2, clicked.height / 2)
        compare(cols.columns[0].selected, 1)
        compare(cols.inspectedUri, "file:///home/t/a.txt")
        // The same objects, not look-alikes built to replace them.
        compare(findChild(cols, "column-0"), column)
        compare(findChild(cols, "colrow-0-1"), clicked)
        compare(findChild(cols, "colrow-0-2"), bystander)
        verify(clicked.sel)
        verify(!bystander.sel)

        mouseClick(bystander, bystander.width / 2, bystander.height / 2)
        compare(cols.inspectedUri, "file:///home/t/b.txt")
        compare(findChild(cols, "column-0"), column)
        verify(bystander.sel && !clicked.sel)
    }

    // Stepping into a folder adds a column, and stepping to a file takes it away again. Neither
    // is a reason to rebuild the column the click was in.
    function test_adding_and_removing_a_column_keeps_the_others() {
        const column = findChild(cols, "column-0")
        const folder = findChild(cols, "colrow-0-0")
        const file = findChild(cols, "colrow-0-2")
        verify(column !== null && folder !== null && file !== null)

        mouseClick(folder, folder.width / 2, folder.height / 2)
        compare(cols.columns.length, 2)
        tryVerify(() => findChild(cols, "column-1") !== null)
        compare(findChild(cols, "column-0"), column)
        compare(findChild(cols, "colrow-0-0"), folder)
        compare(findChild(cols, "colrow-0-2"), file)

        wait(600)   // or the second click reads as the back half of a double click
        mouseClick(file, file.width / 2, file.height / 2)
        compare(cols.columns.length, 1)
        tryVerify(() => findChild(cols, "column-1") === null)
        compare(findChild(cols, "column-0"), column)
        compare(findChild(cols, "colrow-0-0"), folder)
        compare(findChild(cols, "colrow-0-2"), file)
    }

    // The info column follows the selection; there is nothing to close it to.
    function test_the_info_column_has_no_close_box() {
        const row = findChild(cols, "colrow-0-1")
        mouseClick(row, row.width / 2, row.height / 2)
        const box = findChild(cols, "inspector-close")
        verify(box !== null)
        verify(!box.visible)
    }

    // ---------------------------------------------------------------- what is chosen

    // The key column — the one last clicked — and the row highlighted there. Operations read this
    // rather than the pane's own selection, which knows nothing about the columns.
    function test_the_key_columns_highlighted_row_is_what_is_chosen() {
        compare(cols.selectedUris(), [], "nothing highlighted, nothing chosen")
        const row = findChild(cols, "colrow-0-1")                  // a.txt
        mouseClick(row, row.width / 2, row.height / 2)
        compare(cols.selectedUris(), ["file:///home/t/a.txt"])
    }

    function test_a_row_in_a_deeper_column_is_chosen_where_it_lives() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        const folder = findChild(cols, "colrow-0-0")
        mouseClick(folder, folder.width / 2, folder.height / 2)
        compare(cols.selectedUris(), ["file:///home/t/Projects"], "the folder stepped into is chosen")
        tryVerify(() => findChild(cols, "colrow-1-0") !== null)
        const deep = findChild(cols, "colrow-1-0")
        wait(600)                                   // not the back half of a double click
        mouseClick(deep, deep.width / 2, deep.height / 2)
        compare(cols.focusCol, 1)
        compare(cols.selectedUris(), ["file:///home/t/Projects/deep.txt"],
                "a file the pane is not standing in front of")
    }

    /// The pill drawn behind a chosen row.
    function mark(row) { return findChild(row, "rowmark") }

    // Drilling right: the row last clicked keeps the accent — it is the key column — and the
    // trail behind it stays chosen in grey, so the path drilled is still readable.
    function test_the_key_column_holds_the_accent_and_the_trail_goes_grey() {
        fake.tree["file:///home/t/Projects"] = [fake.dir("src"), fake.file("deep.txt")]
        fake.tree["file:///home/t/Projects/src"] = [fake.file("main.rs")]
        const folder = findChild(cols, "colrow-0-0")               // Projects
        mouseClick(folder, folder.width / 2, folder.height / 2)
        verify(folder.sel && folder.active, "the row just clicked is the key one")
        verify(Qt.colorEqual(mark(folder).color, Kiki.Theme.accent), String(mark(folder).color))

        tryVerify(() => findChild(cols, "colrow-1-0") !== null)
        wait(600)                                   // not the back half of a double click
        const sub = findChild(cols, "colrow-1-0")                  // src, one column right
        mouseClick(sub, sub.width / 2, sub.height / 2)
        compare(cols.focusCol, 1)
        verify(sub.sel && sub.active, "the key column moved right with the click")
        verify(Qt.colorEqual(mark(sub).color, Kiki.Theme.accent), String(mark(sub).color))
        verify(folder.sel && !folder.active, "the folder behind is still chosen, no longer key")
        verify(Qt.colorEqual(mark(folder).color, Kiki.Theme.surface), String(mark(folder).color))
        verify(Qt.colorEqual(mark(findChild(cols, "colrow-1-1")).color, "transparent"),
               "a row nobody chose has no pill at all")
    }

    // ---------------------------------------------------------------- dragging a row out

    // The offscreen platform ends a drag as it begins, so what is asserted is that one began and
    // what it carried.
    Component { id: spyC; SignalSpy { signalName: "dragStarted" } }
    /// Press on `item` at (x, y), move by (dx, dy) in steps with the button held, let go.
    function pull(item, x, y, dx, dy) {
        mousePress(item, x, y, Qt.LeftButton)
        for (let k = 1; k <= 6; k++) mouseMove(item, x + dx * k / 6, y + dy * k / 6, -1, Qt.LeftButton)
        wait(50)                                   // an automatic drag starts from a queued event
        mouseRelease(item, x + dx, y + dy, Qt.LeftButton)
    }

    function test_a_dragged_row_carries_its_uri_and_becomes_the_marked_one() {
        const row = findChild(cols, "colrow-0-2")                  // b.txt
        const proxy = findChild(row, "coldrag")
        const spy = spyC.createObject(tc, { target: proxy.Drag })
        pull(row, row.width / 2, row.height / 2, 40, -40)
        compare(spy.count, 1, "a drag began")
        compare(proxy.Drag.mimeData["text/uri-list"], "file:///home/t/b.txt\r\n")
        compare(cols.columns[0].selected, 2, "the row being dragged is the marked one")
        compare(cols.inspectedUri, "", "and a drag opens nothing")
        spy.destroy()
    }

    // A column's rows can belong to a folder the pane is not standing in, so the URI is the
    // column's own, not the pane's.
    function test_a_row_in_a_deeper_column_drags_its_own_folders_file() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        const folder = findChild(cols, "colrow-0-0")
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => findChild(cols, "colrow-1-0") !== null)
        const row = findChild(cols, "colrow-1-0")
        const proxy = findChild(row, "coldrag")
        const spy = spyC.createObject(tc, { target: proxy.Drag })
        pull(row, row.width / 2, row.height / 2, 40, -40)
        compare(spy.count, 1, "a drag began")
        compare(proxy.Drag.mimeData["text/uri-list"], "file:///home/t/Projects/deep.txt\r\n")
        compare(cols.columns[1].selected, 0)
        spy.destroy()
    }

    // ---------------------------------------------------------------- git (plan 15)

    function columnRows(colIndex) {
        const out = []
        const walk = it => { for (const c of it.children) { if (c.objectName === "git-badge") out.push(c); walk(c) } }
        walk(cols)
        return out
    }
    function badges() { return columnRows().filter(b => b.visible).map(b => b.text).sort() }

    // A column is a listing like any other and its rows carry `git`; the view drew none of it, so
    // a modified file looked clean here and nowhere else.
    function test_a_column_row_shows_its_git_state_like_a_list_row() {
        const mod = fake.file("a.txt"); mod.git = { state: "modified", staged: false }
        const unt = fake.file("b.txt"); unt.git = { state: "untracked", staged: false }
        const dir = fake.dir("Projects"); dir.git = { state: "modified", staged: false }
        fake.tree = { "file:///home/t": [dir, mod, unt], "file:///home/t/Projects": [Object.assign(fake.file("main.rs"), { git: { state: "added", staged: true } })] }
        pane.open("file:///home/t"); cols.rebuild(); wait(50)
        compare(badges(), ["?", "M", "M"], "the folder's aggregate, the modified file, the untracked one")
    }

    function test_and_so_does_a_column_opened_from_it() {
        const dir = fake.dir("Projects"); dir.git = { state: "modified", staged: false }
        fake.tree = { "file:///home/t": [dir], "file:///home/t/Projects": [Object.assign(fake.file("main.rs"), { git: { state: "added", staged: true } })] }
        pane.open("file:///home/t"); cols.rebuild(); wait(50)
        const folder = findChild(cols, "colrow-0-0")
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => findChild(cols, "colrow-1-0") !== null)
        tryVerify(() => badges().join(",") === "A,M", 2000, "the opened column's file shows its letter too: " + badges())
    }

    function test_clean_and_ignored_rows_carry_no_letter_and_folders_can_be_switched_off() {
        const ign = fake.file("a.txt"); ign.git = { state: "ignored", staged: false }
        const clean = fake.file("b.txt"); clean.git = { state: "clean", staged: false }
        const dir = fake.dir("Projects"); dir.git = { state: "modified", staged: false }
        fake.tree = { "file:///home/t": [dir, ign, clean] }
        pane.open("file:///home/t"); cols.rebuild(); wait(50)
        compare(badges(), ["M"])
        const was = Kiki.Settings.git
        Kiki.Settings.git = { enabled: true, showIgnored: "dim", folders: "off" }
        compare(badges(), [], "[git] folders = off takes the folder's mark away")
        Kiki.Settings.git = was
    }
}
