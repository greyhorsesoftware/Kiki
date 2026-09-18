import QtQuick
import "." as Kiki

// The file operations of plan 04, as a plain object over one pane: what the menu items, the
// keymap and the IPC all call. It imports nothing from Quickshell, so the interaction tests
// (plan 28) drive the real thing and read back the job each operation submits.
//
// Two things need the window and arrive as signals instead: a confirmation dialog, and putting
// text on the system clipboard.
QtObject {
    id: ops

    property Kiki.Pane pane
    /// URIs cut or copied, and which of the two it was.
    property var clipboard: ({ uris: [], cut: false })
    /// Set while a new folder is on its way, so it can be selected and renamed when it lands.
    property string renameSoon: ""

    /// The shell answers with reply(true) or reply(false).
    signal confirmNeeded(var spec, var reply)
    /// The shell puts this on the system clipboard.
    signal copyText(string text)

    function selectedUris() {
        return pane.selection.positions().map(p => { const r = pane.listing.row(p); return r ? pane.childUri(r.name) : null }).filter(u => u)
    }
    function selectedNames() {
        return pane.selection.positions().map(p => { const r = pane.listing.row(p); return r ? r.name : null }).filter(n => n)
    }

    // ---------------------------------------------------------------- clipboard

    /// Each of these acts on the pane's selection, or on `uris` when a view has its own idea of
    /// what was clicked — columns view, where the row may belong to a folder the pane is not in.
    function copySelection(cut, uris) { const u = uris || selectedUris(); if (u.length) clipboard = { uris: u, cut: !!cut } }
    function paste() {
        if (!clipboard.uris.length) return
        Kiki.Jobs.submit({ op: clipboard.cut ? "move" : "copy", items: clipboard.uris, dest: pane.uri })
        if (clipboard.cut) clipboard = { uris: [], cut: false }
    }
    function copyPath(uris) {
        const u = uris || selectedUris(); if (!u.length) return
        copyText(u.map(x => x.startsWith("file://") ? decodeURIComponent(x.slice(7)) : x).join("\n"))
    }

    // ---------------------------------------------------------------- create, rename, remove

    function newFolder() {
        let name = "New folder", n = 2
        const names = new Set()
        for (let i = 0; i < pane.listing.count; i++) { const r = pane.listing.row(i); if (r) names.add(r.name) }
        while (names.has(name)) name = "New folder " + n++
        // Set before submitting, not in the reply: on a fast filesystem the watcher's Reset can
        // arrive first, and the row would land with nothing waiting to rename it.
        renameSoon = name
        Kiki.Jobs.submit({ op: "mkdir", uri: pane.childUri(name) }, ok => { if (!ok) ops.renameSoon = "" })
    }

    /// Put the current row into the inline editor; only list view has one.
    function renameSelected() {
        if (pane.selection.current < 0) return
        if (pane.view !== "list") pane.view = "list"
        pane.renamingIndex = pane.selection.current
    }

    function trashSelection(uris) { const u = uris || selectedUris(); if (u.length) Kiki.Jobs.submit({ op: "trash", items: u }) }
    function restoreSelection() { const n = selectedNames(); if (n.length) Kiki.Jobs.submit({ op: "restore", names: n }) }

    /// In the trash there is nothing left to lose, so it goes without asking.
    function deleteForever(uris) {
        const u = uris || selectedUris(); if (!u.length) return
        if (pane.isTrash) { Kiki.Jobs.submit({ op: "delete", items: u }); return }
        const what = u.length === 1
            ? decodeURIComponent(u[0].split("/").pop()) + " will be deleted, not moved to the trash. This cannot be undone."
            : u.length + " items will be deleted, not moved to the trash. This cannot be undone."
        confirmNeeded({ title: "Delete permanently?", message: what, label: "Delete" }, yes => { if (yes) Kiki.Jobs.submit({ op: "delete", items: u }) })
    }

    function emptyTrash() {
        confirmNeeded({ title: "Empty the trash?", message: pane.listing.count + " items will be deleted for good.", label: "Empty Trash" },
                      yes => { if (yes) Kiki.Jobs.submit({ op: "emptyTrash" }) })
    }

    // ---------------------------------------------------------------- archives, modes, panes

    function extractHere(name) { Kiki.Jobs.submit({ op: "extract", archive: pane.childUri(name), dest: pane.uri }) }
    /// Extract into a new folder named after the archive, once that folder exists.
    function extractTo(name) {
        const folder = name.replace(/\.(tar\.(gz|xz|zst|bz2)|tgz|txz|tzst|zip|7z|tar)$/i, "")
        Kiki.Jobs.submit({ op: "mkdir", uri: pane.childUri(folder) }, ok => {
            if (ok) Kiki.Jobs.submit({ op: "extract", archive: pane.childUri(name), dest: pane.childUri(folder) })
        })
    }
    function compress(items, archive, format) { Kiki.Jobs.submit({ op: "compress", items: items, archive: archive, format: format }) }
    function chmod(uri, mode, recursive) { Kiki.Jobs.submit({ op: "chmod", items: [uri], mode: mode, recursive: recursive }) }
    /// Copy or move the selection into another pane's folder (plan 07).
    function transferTo(dest, move) {
        const u = selectedUris(); if (!u.length || !dest) return
        Kiki.Jobs.submit({ op: move ? "move" : "copy", items: u, dest: dest })
    }

    /// A rename typed in a row's inline editor.
    property Connections _renames: Connections {
        target: ops.pane
        function onRenameRequested(uri, name) { Kiki.Jobs.submit({ op: "rename", uri: uri, name: name }) }
    }
    /// A new folder lands on a Reset: select it and open the editor on it. The row may not be in
    /// the listing yet, and the view may recycle the delegate out from under the editor while it
    /// is still settling — which closes it again — so this keeps trying until the editor sticks
    /// and gives up after a second rather than spinning.
    function _tryRename() {
        if (!renameSoon) return
        _renameTries++
        if (_renameTries > 25) { renameSoon = ""; return }
        for (let i = 0; i < pane.listing.count; i++) {
            const r = pane.listing.row(i)
            if (r && r.name === renameSoon) {
                pane.selection.set(i)
                renameSelected()
                if (pane.renamingIndex >= 0) renameSoon = ""
                return
            }
        }
    }
    property int _renameTries: 0
    onRenameSoonChanged: if (renameSoon) _renameTries = 0
    property Timer _renameWatch: Timer {
        interval: 40; repeat: true; running: ops.renameSoon !== ""
        onTriggered: ops._tryRename()
    }
    property Connections _created: Connections {
        target: ops.pane ? ops.pane.listing : null
        function onReset() { ops._tryRename() }
    }
}
