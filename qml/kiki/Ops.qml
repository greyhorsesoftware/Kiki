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
    /// Set while a new folder is on its way, so it can be selected and renamed when it lands:
    /// the name it was made under, and the folder it was made in.
    property string renameSoon: ""
    property string renameSoonIn: ""

    /// The shell answers with reply(true) or reply(false).
    signal confirmNeeded(var spec, var reply)
    /// A folder has to be chosen: `spec` is { title, start }, `reply` takes its URI, or nothing.
    signal folderNeeded(var spec, var reply)
    /// The rows of `spec.dest` as some view is showing them, and the way into that view's inline
    /// editor: `reply` takes { count(), row(i), rename(i) }, where `rename` answers whether the
    /// editor stuck. Columns shows several folders at once and answers with the column the folder
    /// is in; with no answer the pane's own listing and the pane's editor stand in, which is what
    /// list, icon and gallery have always used. Ops has no idea which view is showing what, and
    /// this is how it asks rather than reaching into one.
    signal listingNeeded(var spec, var reply)
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
    /// Into the folder on show, or into `dest` when one is named — columns view can ask about a
    /// folder the pane is not standing in. Never into the trash: it is where things go to be
    /// forgotten, not a folder to work in, and the menu there offers neither of these.
    function paste(dest) {
        if (!clipboard.uris.length || pane.isTrash) return
        Kiki.Jobs.submit({ op: clipboard.cut ? "move" : "copy", items: clipboard.uris, dest: dest || pane.uri })
        if (clipboard.cut) clipboard = { uris: [], cut: false }
    }
    function copyPath(uris) {
        const u = uris || selectedUris(); if (!u.length) return
        copyText(u.map(x => x.startsWith("file://") ? decodeURIComponent(x.slice(7)) : x).join("\n"))
    }

    // ---------------------------------------------------------------- create, rename, remove

    function newFolder(dest) {
        if (pane.isTrash) return
        const into = dest || pane.uri
        let name = "New folder", n = 2
        const names = new Set()
        // Only a folder a view is showing has a listing to check the name against; elsewhere the
        // daemon answers with a collision and the prompt handles it. That same view is what puts
        // the new row in the editor, so it is asked for once, here.
        const site = _site(into)
        if (site) for (let i = 0; i < site.count(); i++) { const r = site.row(i); if (r) names.add(r.name) }
        while (names.has(name)) name = "New folder " + n++
        // Set before submitting, not in the reply: on a fast filesystem the watcher's Reset can
        // arrive first, and the row would land with nothing waiting to rename it. A folder made
        // where nobody is looking has no row to rename, so it is left alone.
        renameSoonIn = into
        renameSoon = site ? name : ""
        const uri = into === pane.uri ? pane.childUri(name) : into.replace(/\/+$/, "") + "/" + encodeURIComponent(name)
        Kiki.Jobs.submit({ op: "mkdir", uri: uri }, ok => { if (!ok) ops.renameSoon = "" })
    }

    /// Put the current row into the inline editor; only list view has one.
    function renameSelected() {
        if (pane.selection.current < 0) return
        if (pane.view !== "list") pane.view = "list"
        pane.renamingIndex = pane.selection.current
    }

    /// Del, the menu's Move to Trash, and files dropped on the Trash. A server has no trash, so
    /// what is on one is deleted for good, once the user has been told so; this machine's files
    /// go to the trash without asking.
    function trashSelection(uris) {
        const u = uris || selectedUris(); if (!u.length) return
        const remote = u.filter(x => !/^(file|trash):/.test(x))
        const local = u.filter(x => remote.indexOf(x) < 0)
        if (local.length) Kiki.Jobs.submit({ op: "trash", items: local })
        if (!remote.length) return
        const host = Kiki.Format.authority(remote[0])
        const what = remote.length === 1
            ? decodeURIComponent(remote[0].replace(/\/+$/, "").split("/").pop()) + " is on " + host + ", which has no trash. It will be deleted for good, and this cannot be undone."
            : "These " + remote.length + " items are on " + host + ", which has no trash. They will be deleted for good, and this cannot be undone."
        confirmNeeded({ title: "Delete permanently?", message: what, label: "Delete" }, yes => { if (yes) Kiki.Jobs.submit({ op: "delete", items: remote }) })
    }
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
    /// Ask where, then extract there. What lands is the daemon's to decide, the same as for
    /// "Extract here": the archive's one item, or a folder named after it holding them all, under
    /// a name that is free — never a merge into something already there (plan 05).
    /// `at` names the archive outright when it is not one of the pane's own rows — columns view,
    /// where the row may live in a folder the pane is not standing in; the chooser starts there.
    function extractTo(name, at) {
        const archive = at || pane.childUri(name)
        folderNeeded({ title: "Extract " + name + " to…", start: at ? at.replace(/\/[^/]*$/, "") : pane.uri }, dest => {
            if (dest) Kiki.Jobs.submit({ op: "extract", archive: archive, dest: dest })
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
    /// Who is showing `dest`, and how a row of it is named. Asked again on every try rather than
    /// kept: the columns are rebuilt under a folder that is still being made, and the view itself
    /// can change while the daemon is still working.
    function _site(dest) {
        let site = null
        listingNeeded({ dest: dest }, s => site = s)
        if (site) return site
        // Nobody answered: the pane's own listing, and the editor list view has — which is where
        // a new folder has always been named, and is still where icon and gallery send it.
        if (dest !== pane.uri) return null
        return { count: () => pane.listing.count, row: i => pane.listing.row(i),
                 rename: i => { pane.selection.set(i); renameSelected(); return pane.renamingIndex >= 0 } }
    }
    /// A new folder lands on a Reset: select it and open the editor on it. The row may not be in
    /// the listing yet, and the view may recycle the delegate out from under the editor while it
    /// is still settling — which closes it again — so this keeps trying until the editor sticks
    /// and gives up after a second rather than spinning.
    function _tryRename() {
        if (!renameSoon) return
        _renameTries++
        if (_renameTries > 25) { renameSoon = ""; return }
        const site = _site(renameSoonIn)
        if (!site) return
        for (let i = 0; i < site.count(); i++) {
            const r = site.row(i)
            if (r && r.name === renameSoon) {
                if (site.rename(i)) renameSoon = ""
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
