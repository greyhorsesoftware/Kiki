import QtQuick
import QtQuick.Window
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
        Kiki.Settings.set("view", "columnsWidths", ({})); cols.liveWidths = ({})
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
    // The columns hold the pane's own listing; let go of it before the pane goes — the shell's
    // own columns too, whose caches close themselves against the daemon on the way out.
    function cleanup() {
        shell.pane.view = "list"
        wait(20)
        shell.pane.listing.close()
        // Whichever window a test aimed the keyboard at, the next one starts with this one.
        tc.Window.window.requestActivate()
        cols.columns = []; cols.pane = null
        pane.destroy(); fake.destroy()
    }

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

    // ---------------------------------------------------------------- renaming in place (F2)

    // The editor goes on the row the key column highlights, in the column it lives in. Before
    // this, F2 in Columns switched the pane to list view and renamed whatever the PANE's own
    // selection was — another view's leftover, in another folder.
    SignalSpy { id: renamed; target: tc.pane; signalName: "renameRequested" }
    function editorIn(row) { return findChild(row, "renameEditor") }

    function test_f2_opens_the_editor_on_the_key_columns_row() {
        const row = findChild(cols, "colrow-0-2")                  // b.txt
        mouseClick(row, row.width / 2, row.height / 2)
        cols.beginRename()
        compare(cols.renamingCol, 0)
        compare(cols.renamingIndex, 2)
        const editor = editorIn(row)
        verify(editor !== null && editor.visible, "the editor is on the row that is highlighted")
        compare(editor.text, "b.txt")
        verify(editorIn(findChild(cols, "colrow-0-1")) === null || !editorIn(findChild(cols, "colrow-0-1")).visible,
               "and on no other row")
        cols.cancelRename()
    }

    function test_with_nothing_highlighted_there_is_nothing_to_rename() {
        cols.beginRename()
        compare(cols.renamingIndex, -1)
    }

    // A row in a deeper column belongs to a folder the pane is not standing in, so the rename
    // goes out against that column's URI — not `pane.childUri(name)`.
    function test_the_editor_renames_the_row_in_the_folder_it_lives_in() {
        renamed.target = tc.pane; renamed.clear()
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        const folder = findChild(cols, "colrow-0-0")
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => findChild(cols, "colrow-1-0") !== null)
        const row = findChild(cols, "colrow-1-0")
        wait(600)                                   // not the back half of a double click
        mouseClick(row, row.width / 2, row.height / 2)
        compare(cols.focusCol, 1)
        cols.beginRename()
        const editor = editorIn(row)
        verify(editor !== null && editor.visible)
        editor.forceActiveFocus()
        editor.text = ""
        keyClick(Qt.Key_N); keyClick(Qt.Key_E); keyClick(Qt.Key_W)
        keyClick(Qt.Key_Return)
        compare(renamed.count, 1)
        compare(renamed.signalArguments[0][0], "file:///home/t/Projects/deep.txt")
        compare(renamed.signalArguments[0][1], "new")
        compare(cols.renamingIndex, -1, "and the editor closes behind it")
    }

    function test_escape_leaves_the_name_alone() {
        renamed.target = tc.pane; renamed.clear()
        const row = findChild(cols, "colrow-0-1")
        mouseClick(row, row.width / 2, row.height / 2)
        cols.beginRename()
        const editor = editorIn(row)
        editor.forceActiveFocus()
        editor.text = "something else"
        keyClick(Qt.Key_Escape)
        compare(renamed.count, 0)
        compare(cols.renamingIndex, -1)
    }

    // ---------------------------------------------------------------- the menu, and a new folder

    // These need the window: the menu is the shell's, and so is the route from Ctrl+Shift+N into
    // the column the user is working in. The shell builds a columns view of its own over its own
    // pane, and that one — not `cols` — is what these drive.
    Kiki.Shell { id: shell; width: 1200; height: 760; visible: true }
    SignalSpy { id: viewChanges; target: shell.pane; signalName: "viewChanged" }
    function shellCols() { return shell.columnsPane() }
    /// The shell showing `uri` in columns, on the same fake daemon the rest of the file uses.
    function openInShell(uri) {
        shell.left.listing.daemon = fake
        shell.pane.open(uri)
        wait(50)
        shell.pane.view = "columns"
        tryVerify(() => tc.shellCols() !== null)
        wait(50)
        viewChanges.clear()
        Wire.reset()
    }
    function shellRow(col, index) { return findChild(shellCols(), "colrow-" + col + "-" + index) }
    /// Pick a row out of the menu the shell would put up for what is chosen now — the Menu key's
    /// list, and the one the `contextMenu` IPC acts through.
    function chooseInMenu(label) {
        const it = shell.contextItemsNow().find(i => i.label === label)
        verify(it && it.action, label + " is in the menu")
        it.action()
    }
    /// The mkdir the shell just submitted, answered as the daemon answers it: the folder appears
    /// in `dir`, and the listing that column is on is told to reset.
    function landNewFolder(dir, cache) {
        const req = Wire.last("Submit")
        verify(req && req.op.op === "mkdir", "a mkdir went out")
        fake.tree[dir] = (fake.tree[dir] || []).concat([fake.dir("New folder")])
        Wire.reply(req.id, { job: 1 })
        fake.emitEvent({ event: "Reset", lid: cache.lid, n: fake.tree[dir].length })
        return req
    }

    // The row menu had no Rename because columns had no editor to open; it has one now, and the
    // row that menu belongs to is what it renames.
    function test_the_row_menu_offers_rename_where_list_view_has_it() {
        const row = fake.file("a.txt")
        const labels = shell.contextItemsForUri("file:///home/t/a.txt", row).map(i => i.label)
        const at = labels.indexOf("Rename")
        verify(at >= 0, "the row menu has a Rename: " + labels)
        compare(shell.contextItemsForUri("file:///home/t/a.txt", row)[at].key, "F2", "keyed as list view's is")
        // The same neighbour it has in the pane's own menu, so the item does not move about
        // between the two menus a right click can give.
        const own = shell.contextItems(-1).map(i => i.label)
        compare(labels[at + 1], own[own.indexOf("Rename") + 1], "and in the same place among its neighbours")
    }

    // Two menus for the same thing, written out twice, and the columns one had fallen behind: no
    // Paste, no New folder, no "Extract to…", and none of the ways of sending (owner, 2026-09-21:
    // "columns view does not have the send with tail and mail items"). Row for row, in order,
    // they are the same list now — and this fails the day one of them gains a row alone.
    function test_the_row_menu_is_list_views_menu_row_for_row() {
        shell.sharePlugins = [{ id: "mail", name: "Mail", icon: "mail", targets: "none" }, { id: "tailscale", name: "Tailscale", icon: "share", targets: "list" }]
        const row = fake.file("a.txt")
        const mine = shell.contextItemsForUri("file:///home/t/a.txt", row)
        const own = shell.contextItems(-1)
        compare(mine.map(i => i.label), own.map(i => i.label))
        compare(mine.map(i => i.key || ""), own.map(i => i.key || ""), "keyed alike")
        compare(mine.map(i => i.sep === true), own.map(i => i.sep === true), "and divided alike")
        // The ways of sending are live on a row here, where the pane's own menu with nothing
        // selected greys them: they send the row the menu was raised over.
        const send = mine.filter(i => i.label.indexOf("Send via ") === 0)
        compare(send.map(i => i.label), ["Send via Mail", "Send via Tailscale"])
        verify(send.every(i => i.enabled !== false), "and they are live")
        shell.sharePlugins = []
    }

    // "Extract to…" asks where, starting in the folder the archive is IN — which in columns need
    // not be the pane's — and extracts that archive, not a row of the pane's by the same name.
    function test_extract_to_on_a_deeper_row_is_about_that_row() {
        const row = Object.assign(fake.file("site.zip"), { kind: "archive" })
        const item = shell.contextItemsForUri("file:///home/t/Projects/site.zip", row).find(i => i.label === "Extract to…")
        verify(item && item.enabled !== false)
        item.action()
        const portal = findChild(shell, "portal")
        verify(portal && portal.visible, "the chooser is asked")
        compare(portal.req.currentFolder, "/home/t/Projects", "starting where the archive is")
        portal.finish(["file:///home/t/out"])
        const req = Wire.last("Submit")
        compare(req.op.op, "extract"); compare(req.op.archive, "file:///home/t/Projects/site.zip"); compare(req.op.dest, "file:///home/t/out")
    }

    // Right-clicking a row makes it the column's highlighted one, as it does in a list — so the
    // menu's Rename opens the editor on the row the pointer was over, in the column it lives in,
    // and not on whatever was highlighted before.
    function test_the_menus_rename_opens_the_editor_on_the_row_it_was_raised_over() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt"), fake.file("other.txt")]
        openInShell("file:///home/t")
        const folder = shellRow(0, 0)
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => tc.shellRow(1, 1) !== null)
        wait(600)                                   // not the back half of a double click
        const chosen = shellRow(1, 0)               // deep.txt: the highlighted row
        mouseClick(chosen, chosen.width / 2, chosen.height / 2)
        compare(shellCols().columns[1].selected, 0)

        wait(600)
        const other = shellRow(1, 1)                // other.txt: right-clicked, not highlighted
        mouseClick(other, other.width / 2, other.height / 2, Qt.RightButton)
        compare(shellCols().columns[1].selected, 1, "the right click chose the row it was over")
        chooseInMenu("Rename")
        compare(shellCols().renamingCol, 1, "the editor is in the column the row lives in")
        compare(shellCols().renamingIndex, 1, "on the row the menu was raised over")
        const editor = editorIn(other)
        verify(editor !== null && editor.visible)
        compare(editor.text, "other.txt")
        compare(viewChanges.count, 0, "and the view is still Columns")
        shellCols().cancelRename()
    }

    // Ctrl+Shift+N used to make the folder in the folder the PANE was on and then switch to list
    // view for an editor to name it in — the columns thrown away, and the folder made behind the
    // one on screen.
    function test_a_new_folder_lands_in_the_key_column_ready_to_be_named() {
        openInShell("file:///home/t")
        shell.runAction("newFolder")
        const req = landNewFolder("file:///home/t", shellCols().columns[0].cache)
        compare(req.op.uri, "file:///home/t/New%20folder")
        tryVerify(() => tc.shellCols() && tc.shellCols().renamingIndex >= 0, 2000,
                 "the editor opens on it when it lands, without leaving Columns to find one")
        const cols = shellCols()
        compare(cols.renamingCol, 0)
        compare(cols.columns[0].cache.row(cols.renamingIndex).name, "New folder")
        compare(cols.columns[0].selected, cols.renamingIndex, "and it is what the column highlights")
        compare(shell.pane.renamingIndex, -1, "the pane's own editor is left out of it")
        compare(viewChanges.count, 0, "the view never changes")
        const editor = editorIn(shellRow(0, cols.renamingIndex))
        verify(editor !== null && editor.visible)
        compare(editor.selectedText, "New folder", "with the name ready to be typed over")
        cols.cancelRename()
    }

    // The key column need not be the pane's: a folder made from a deeper column belongs to THAT
    // column's folder, and is named there.
    function test_a_new_folder_in_a_deeper_column_is_made_in_that_columns_folder() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        openInShell("file:///home/t")
        const folder = shellRow(0, 0)
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => tc.shellRow(1, 0) !== null)
        shellCols().focusRight()                    // the key column is Projects' own now
        compare(shellCols().focusCol, 1)

        shell.runAction("newFolder")
        const req = landNewFolder("file:///home/t/Projects", shellCols().columns[1].cache)
        compare(req.op.uri, "file:///home/t/Projects/New%20folder", "in the key column's folder")
        tryVerify(() => tc.shellCols().renamingCol === 1, 2000, "and named in that column")
        const cols = shellCols()
        compare(cols.columns[1].cache.row(cols.renamingIndex).name, "New folder")
        compare(cols.columns[1].selected, cols.renamingIndex)
        compare(shell.pane.uri, "file:///home/t", "while the pane stands in the folder above")
        compare(viewChanges.count, 0, "the view never changes")
        cols.cancelRename()
    }

    // You clicked Projects and asked for a new folder: it is one of Projects' (owner, 2026-09-21).
    // The accent is still on the column Projects is IN — it used to be made there, beside it.
    function test_a_new_folder_lands_inside_the_folder_that_is_highlighted() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        openInShell("file:///home/t")
        const folder = shellRow(0, 0)
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => tc.shellRow(1, 0) !== null)
        compare(shellCols().focusCol, 0, "the key column is still the one Projects is in")
        compare(shellCols().newFolderUri(), "file:///home/t/Projects")

        shell.runAction("newFolder")
        const req = landNewFolder("file:///home/t/Projects", shellCols().columns[1].cache)
        compare(req.op.uri, "file:///home/t/Projects/New%20folder", "inside the highlighted folder, not beside it")
        tryVerify(() => tc.shellCols().renamingCol === 1, 2000, "and named in the column that folder opens")
        compare(viewChanges.count, 0, "the view never changes")
        shellCols().cancelRename()
    }

    // A highlighted FILE is not somewhere to put a folder: the key column's own folder takes it.
    function test_a_highlighted_file_sends_the_new_folder_to_the_key_columns_folder() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        openInShell("file:///home/t")
        const folder = shellRow(0, 0)
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => tc.shellRow(1, 0) !== null)
        const file = shellRow(1, 0)
        mouseClick(file, file.width / 2, file.height / 2)
        compare(shellCols().focusCol, 1)
        compare(shellCols().newFolderUri(), "file:///home/t/Projects")
    }

    // A column's own menu belongs to the folder it was raised over: a right click on the empty
    // space of a column BEHIND the key one makes its folder there. (Ctrl+Shift+N goes to the key
    // column, and this menu item used to be left to do the same.)
    function test_the_column_menus_new_folder_belongs_to_that_column() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        openInShell("file:///home/t")
        const folder = shellRow(0, 0)
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => tc.shellRow(1, 0) !== null)
        shellCols().focusRight()
        compare(shellCols().focusCol, 1, "the key column is the deeper one")

        const it = shell.folderItems("file:///home/t").find(i => i.label === "New folder")
        verify(it && it.action, "the folder menu offers New folder")
        it.action()
        compare(Wire.last("Submit").op.uri, "file:///home/t/New%20folder",
                "in the folder the menu was raised over, not the key column's")
    }

    // Escape is "leave it alone": the folder keeps the name it was made under, and nothing is
    // renamed behind it.
    function test_escape_leaves_a_new_folder_called_new_folder() {
        openInShell("file:///home/t")
        shell.runAction("newFolder")
        landNewFolder("file:///home/t", shellCols().columns[0].cache)
        tryVerify(() => tc.shellCols() && tc.shellCols().renamingIndex >= 0, 2000, "it is in the editor")
        const cols = shellCols()
        const at = cols.renamingIndex
        const editor = editorIn(shellRow(0, at))
        // The editor is in the shell's own window, so the keystroke has to be aimed at it; the
        // test window takes the keyboard back in cleanup().
        shell.requestActivate()
        editor.forceActiveFocus()
        wait(50)
        editor.text = "something else"
        keyClick(Qt.Key_Escape)
        compare(cols.renamingIndex, -1, "the editor closes")
        compare(Wire.last("Submit").op.op, "mkdir", "and no rename goes out behind it")
        compare(cols.columns[0].cache.row(at).name, "New folder")
        compare(viewChanges.count, 0)
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

    // ---------------------------------------------------------------- a column dragged to a width
    function forgetWidths() { Kiki.Settings.set("view", "columnsWidths", ({})); cols.liveWidths = ({}) }
    function twoColumns() {
        fake.tree["file:///home/t/Projects"] = [fake.file("deep.txt")]
        const folder = findChild(cols, "colrow-0-0")
        mouseClick(folder, folder.width / 2, folder.height / 2)
        tryVerify(() => findChild(cols, "column-1") !== null && cols.columns.length === 2)
        // The Row places its columns on the next frame; until then the new one sits on the first.
        tryVerify(() => findChild(cols, "column-1").x > 0)
    }
    function test_the_line_between_two_columns_drags_the_one_on_its_left() {
        forgetWidths()
        twoColumns()
        const first = findChild(cols, "column-0"), second = findChild(cols, "column-1"), grip = findChild(cols, "column-grip-0")
        compare(first.width, second.width, "until one is dragged they share the room")
        const was = first.width
        Wire.reset()
        mousePress(grip, 3, 50)
        mouseMove(grip, 3 - 60, 50)
        compare(first.width, was - 60)
        tryCompare(second, "x", first.width, 1000, "the next column starts where this one ends")
        compare(Wire.count("SetSettings"), 0, "settings.toml is written when the button comes up, not on every pixel")
        mouseRelease(grip, 3, 50)
        compare(Wire.count("SetSettings"), 1)
        compare(Wire.last("SetSettings").patch.view.columnsWidths.c0, was - 60)
        // What was taken from the first goes to the one still sharing.
        compare(second.width, cols.width - first.width)
        // Never narrower than a name can be read in.
        mousePress(grip, 3, 50); mouseMove(grip, 3 - 2000, 50); mouseRelease(grip, 3, 50)
        compare(first.width, cols.columnWidthMin)
        // A double click lets the width go again.
        mouseDoubleClickSequence(grip, 3, 50)
        tryCompare(first, "width", second.width)
        verify(Wire.last("SetSettings").patch.view.columnsWidths.c0 === null, "the daemon is told to forget it, or it is back at the next start")
        forgetWidths()
    }
    function test_a_width_belongs_to_the_place_not_the_folder() {
        forgetWidths()
        cols.setColumnWidth(0, 333); cols.endColumnResize()
        fake.tree["file:///home/u"] = [fake.file("other.txt")]
        pane.open("file:///home/u"); cols.rebuild(); wait(50)
        compare(findChild(cols, "column-0").width, 333)
        forgetWidths()
    }

    // ---------------------------------------------------------------- the wheel, sideways
    // A mouse has one wheel. Shift turns it sideways anywhere; over a column with nothing of its
    // own to scroll it walks the columns without Shift.
    function wideStrip() {
        forgetWidths()
        twoColumns()
        cols.setColumnWidth(0, 800); cols.setColumnWidth(1, 800)
        verify(cols.stripWidth > cols.width)
        cols.sideways(-100000)
        compare(cols.scrollX, 0)
    }
    function test_shift_and_the_wheel_walks_the_columns() {
        wideStrip()
        const c0 = findChild(cols, "column-0")
        mouseWheel(c0, 400, 200, 0, 240, Qt.NoButton, Qt.ShiftModifier)
        verify(cols.scrollX > 0, "Shift+wheel did not move the strip")
        const there = cols.scrollX
        mouseWheel(c0, 400, 200, 0, -240, Qt.NoButton, Qt.ShiftModifier)
        verify(cols.scrollX !== there, "and the other way brings it back")
        forgetWidths()
    }
    function test_the_plain_wheel_over_a_column_that_fits_walks_them_too() {
        wideStrip()
        const c0 = findChild(cols, "column-0")
        mouseWheel(c0, 400, 200, 0, 240)
        verify(cols.scrollX > 0, "three rows have nothing to scroll: the wheel belongs to the strip")
        forgetWidths()
    }
    function test_a_column_with_rows_to_scroll_keeps_the_wheel() {
        const rows = []
        for (let i = 0; i < 200; i++) rows.push(fake.file("f" + String(i).padStart(3, "0") + ".txt"))
        fake.tree["file:///home/long"] = rows
        pane.open("file:///home/long"); cols.rebuild(); wait(80)
        cols.setColumnWidth(0, 2000 > cols.columnWidthMax ? cols.columnWidthMax : 2000)
        const c0 = findChild(cols, "column-0")
        const x = cols.scrollX
        mouseWheel(c0, 400, 200, 0, -240)
        compare(cols.scrollX, x, "the wheel scrolled the files, not the strip")
        forgetWidths()
    }

    // ---------------------------------------------------------------- the path follows the columns
    // The pane stays on the first column's folder however far the columns go, so the path over the
    // view read "t" with Projects → deep → deeper open under it (owner, 2026-09-21).
    function threeDeep() {
        fake.tree["file:///home/t/Projects"] = [fake.dir("deep"), fake.file("p.txt")]
        fake.tree["file:///home/t/Projects/deep"] = [fake.dir("deeper")]
        fake.tree["file:///home/t/Projects/deep/deeper"] = [fake.file("bottom.txt")]
        cols.select(0, 0); tryVerify(() => cols.columns.length === 2 && cols.columns[1].cache.count === 2)
        cols.select(1, 0); tryVerify(() => cols.columns.length === 3 && cols.columns[2].cache.count === 1)
        cols.select(2, 0); tryVerify(() => cols.columns.length === 4)
    }
    function test_the_shown_folder_is_the_deepest_column() {
        compare(cols.shownUri, "file:///home/t")
        threeDeep()
        compare(cols.shownUri, "file:///home/t/Projects/deep/deeper")
        compare(pane.uri, "file:///home/t", "the pane itself has not moved")
        // A file chosen opens no column: the path is still its folder's.
        cols.select(1, 1)
        compare(cols.shownUri, "file:///home/t/Projects")
    }
    function test_a_pill_that_is_a_column_brings_the_columns_back_to_it() {
        threeDeep()
        verify(cols.backTo("file:///home/t/Projects"))
        compare(cols.columns.length, 2)
        compare(cols.shownUri, "file:///home/t/Projects")
        compare(cols.focusCol, 1)
        compare(cols.columns[1].selected, -1, "nothing in it chosen: it is the folder that was asked for")
        compare(cols.columns[0].selected, 0, "and the trail to it keeps its highlight")
        compare(pane.uri, "file:///home/t")
        // The first column is a column too; a trailing slash is the same folder.
        verify(cols.backTo("file:///home/t/"))
        compare(cols.columns.length, 1)
    }
    function test_a_pill_above_the_first_column_is_not_the_columns_to_answer() {
        threeDeep()
        verify(!cols.backTo("file:///home"))
        compare(cols.columns.length, 4, "and nothing was taken down for it")
    }

    // The same, through the window: the title bar's path is the one the owner was looking at.
    function test_the_title_bars_path_follows_the_columns_and_its_pills_bring_them_back() {
        fake.tree["file:///home/t/Projects"] = [fake.dir("deep")]
        fake.tree["file:///home/t/Projects/deep"] = [fake.file("bottom.txt")]
        openInShell("file:///home/t")
        const crumb = findChild(shell.contentItem, "toolbar").breadcrumb, c = shellCols()
        compare(crumb.uri, "file:///home/t")
        c.select(0, 0); tryVerify(() => c.columns.length === 2 && c.columns[1].cache.count === 1)
        c.select(1, 0); tryVerify(() => c.columns.length === 3)
        compare(crumb.uri, "file:///home/t/Projects/deep")
        Wire.reset()
        crumb.navigate("file:///home/t/Projects")           // a pill that is one of the columns
        compare(shellCols().columns.length, 2)
        compare(crumb.uri, "file:///home/t/Projects")
        compare(shell.pane.uri, "file:///home/t", "the columns came back; the pane did not start again from there")
        crumb.navigate("file:///home")                       // one above them all: the pane opens it
        tryCompare(shell.pane, "uri", "file:///home")
        // List view has no columns to follow: the path is the pane's again.
        shell.pane.view = "list"
        tryCompare(crumb, "uri", "file:///home")
    }
}
