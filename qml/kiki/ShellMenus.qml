import QtQuick
import Quickshell
import "." as Kiki
import "viewmenu.js" as ViewMenu

// The window's menus, split out of Shell.qml (docs/0.5.0/04-shell-split.md): the tables of rows
// — a file's context menu, a folder's, the gear, the folded toolbar, the view switcher, Open
// with, the path's ancestors, the mirror bar's — and the one function that hangs any of them
// under a button. Each row's `action` calls the function on `win` the click calls; the rows
// and ticks of the view menu stay `viewmenu.js`'s.
QtObject {
    id: menus
    required property var win

    function mirrorOptions(pos) {
        win.menu.open([
            { label: Kiki.T.tr("menu.mirrorUpload", { host: (win.remoteUri().match(/^[a-z]+:\/\/([^/]+)/) || [])[1] }), key: "Ctrl+M", action: () => win.startMirror(true) },
            { label: Kiki.T.tr("menu.mirrorDownload", { host: (win.remoteUri().match(/^[a-z]+:\/\/([^/]+)/) || [])[1] }), action: () => win.startMirror(false) },
            { id: "swapSides", label: Kiki.T.tr("menu.swapSides"), sep: true, action: () => win.swapPanes() },
            { id: "openRemoteAlone", label: Kiki.T.tr("menu.openRemoteAlone"), action: () => { const r = win.remoteUri(); win.left.view = "list"; win.left.open(r) } },
        ], pos)
    }

    /// A menu hung under `item`. The toolbar's buttons stand at the right edge, so their menus
    /// hang with their right edge on the button's (owner, 2026-09-23); the path's hang from the
    /// left, where the crumb is. `place()` still keeps the box inside the window either way.
    function menuUnder(item, items, alignRight) {
        const p = item.mapToItem(win.menu.parent, alignRight ? item.width - win.menu.box.width : 0, item.height + 4)
        win.menu.open(items, Qt.point(p.x, p.y))
    }
    // Clicking the path offers the folders above this one, and the way into typing one.
    function pathMenu() {
        const crumb = win.activeCrumb()
        const items = crumb.ancestors().map(a => ({ label: a.label, action: () => win.pane.open(a.uri) }))
        items.push({ id: "typePath", label: Kiki.T.tr("menu.typePath"), key: "Ctrl+L", sep: items.length > 0, action: () => crumb.edit() })
        menuUnder(crumb, items)
    }
    /// The same menu for a pane's own header: the folders above, and typing a path.
    function paneHeaderPathMenu(target, crumb) {
        const items = crumb.ancestors().map(a => ({ label: a.label, action: () => target.open(a.uri) }))
        items.push({ id: "typePath", label: Kiki.T.tr("menu.typePath"), sep: items.length > 0, action: () => crumb.edit() })
        menuUnder(crumb, items)
    }
    /// The gear: settings, the keymap, and who made this.
    function gearItems() {
        return [
            { id: "settings", label: Kiki.T.tr("menu.settings"), key: win.keymap.chordFor("settings"), action: () => win.settingsWin.open("general") },
            { id: "shortcuts", label: Kiki.T.tr("menu.shortcuts"), key: win.keymap.chordFor("shortcuts"), action: () => win.keysWin.open() },
            { id: "about", label: Kiki.T.tr("menu.about"), sep: true, action: () => win.aboutDlg.open() },
        ]
    }
    function gearMenu() { menuUnder(win.toolbar.gearButton, gearItems(), true) }
    /// The toolbar folded: every button it hides, as one menu — the views as a submenu, the
    /// gear's rows at the bottom.
    function hamburgerItems() {
        const items = [
            { label: win.inspectorRequested ? Kiki.T.tr("menu.hideInfo") : Kiki.T.tr("menu.showInfo"), key: win.keymap.chordFor("inspector"), enabled: win.pane.view !== "columns" && win.inspectedUri !== "", action: () => win.inspectorRequested = !win.inspectorRequested },
        ]
        if (win.remoteOpen || win.split) items.push({ label: win.split ? Kiki.T.tr("menu.onePane") : Kiki.T.tr("menu.sideBySide"), key: win.keymap.chordFor("viewMirror"), action: () => win.toggleMirrorView() })
        items.push({ id: "view", label: Kiki.T.tr("menu.view"), sep: true, items: viewMenuItems() })
        items.push({ label: win.sidebarShown ? Kiki.T.tr("menu.hideFavorites") : Kiki.T.tr("menu.showFavorites"), key: win.keymap.chordFor("sidebar"), action: () => win.sidebarShown = !win.sidebarShown })
        const gear = gearItems(); gear[0].sep = true
        return items.concat(gear)
    }
    function hamburgerMenu(button) { menuUnder(button, hamburgerItems(), true) }
    /// The rows and their ticks are `viewmenu.js`'s (tested there); what each does is here.
    /// Ticked for the FOCUSED pane's view, side by side as well — each pane has its own. (The
    /// ticks used to be withheld when split: a leftover from when Mirror was itself a view and
    /// none of these was the current one.)
    function viewMenuItems() {
        const act = { gallery: () => win.enterGallery(), hidden: () => win.pane.setHidden(!win.pane.showHidden) }
        return ViewMenu.items(win.pane.view, win.pane.showHidden, Kiki.T.tr, win.pane.isLocal).map(it => Object.assign(it, { action: act[it.id] || (() => win.setView(it.id)) }))
    }
    function viewMenu() {
        const items = viewMenuItems()
        menuUnder(win.toolbar.viewButton, items, true)
    }

    // Open with (plans 02/03 and 14): one list holding the desktop entries for the file's MIME
    // type and kiki's own tools, so there is a single way to open something elsewhere.
    /// Open with is the desktop's applications. The terminal tools and agents of plan 14 are
    /// their own thing (`Alt+Enter`, the editor keys) and do not belong in a list of apps.
    function openWithItems() { return [{ id: "looking", label: Kiki.T.tr("menu.looking"), enabled: false, action: () => {} }] }
    function loadOpenWith(uris, apply) {
        apply(openWithItems())
        // A selection is offered what opens all of it, and the app is handed the lot.
        Kiki.Daemon.request("OpenWith", { uris: uris }, (ok, err) => {
            if (!ok) { apply([{ label: err ? err.message : "Nothing offered", enabled: false, action: () => {} }]); return }
            const apps = ok.apps.map(a => ({ label: a.name + (a.default ? "  ·  default" : ""), icon: "open",
                                             action: () => Kiki.Daemon.open(uris, "app:" + a.id, {},
                                                 (r, e) => { if (e) Kiki.Jobs.showToast({ text: e.message, undoable: false }) }) }))
            apply(apps.length ? apps : [{ label: uris.length > 1 && !ok.mime ? "No application opens all of these" : "No application for this kind", enabled: false, action: () => {} }])
        })
    }
    function openWithMenu(pos) {
        const u = win.selectedUris(); if (!u.length) return
        loadOpenWith(u, list => {
            const items = list.length ? list : [{ id: "nothingToOpenWith", label: Kiki.T.tr("menu.nothingToOpenWith"), enabled: false, action: () => {} }]
            if (win.menu.visible) win.menu.items = items
            else if (pos) win.menu.open(items, pos)
            else menuUnder(win.toolbar.viewButton, items, true)
        })
    }

    property var openWithSub: []
    /// The menu for one row addressed by URI rather than by a position in the pane's listing:
    /// columns view shows several folders at once, so the row that was clicked need not be in
    /// the folder the pane is on. Same actions, aimed at that one file.
    function contextItemsForUri(uri, row) {
        const uris = [uri]
        openWithSub = openWithItems()
        loadOpenWith(uris, list => { openWithSub = list; if (win.menu.visible) win.menu.refill("openWith", list) })
        const folder = uri.replace(/\/[^/]*$/, "")
        return [
            { id: "open", label: Kiki.T.tr("menu.open"), key: "Enter", action: () => row && row.isDir ? win.pane.open(uri) : win.openExternal(uri) },
            { id: "openWith", label: Kiki.T.tr("menu.openWith"), items: openWithSub },
            { id: "getInfo", label: Kiki.T.tr("menu.getInfo"), key: "Ctrl+I", action: () => { win.inspectedUri = uri; win.inspectedRow = row; win.inspectorRequested = true } },
            { id: "copy", label: Kiki.T.tr("menu.copy"), key: "Super+C", sep: true, action: () => win.ops.copySelection(false, uris) },
            { id: "cut", label: Kiki.T.tr("menu.cut"), key: "Super+X", action: () => win.ops.copySelection(true, uris) },
            // The same rows, in the same order, as list view's menu (`contextItems`) — this one
            // had fallen behind it: no Paste, no New folder, no "Extract to…" and none of the
            // ways of sending. Paste goes into the folder the row is in; New folder goes where
            // Ctrl+Shift+N does, inside the row when the row is a folder.
            { id: "paste", label: Kiki.T.tr("menu.paste"), key: "Super+V", enabled: win.clipboard.uris.length > 0, action: () => win.ops.paste(folder) },
            // On the row the menu was raised over: the right click made that row the column's
            // highlighted one, as a right click does in a list.
            { id: "newFolder", label: Kiki.T.tr("menu.newFolder"), key: "Ctrl+Shift+N", sep: true, action: () => win.newFolder() },
            { id: "rename", label: Kiki.T.tr("menu.rename"), key: "F2", action: () => win.renameSelected() },
            { id: "compress", label: Kiki.T.tr("menu.compress"), action: () => win.compressDialog.open(uris, folder) },
            { id: "extractHere", label: Kiki.T.tr("menu.extractHere"), enabled: !!row && row.kind === "archive", action: () => Kiki.Jobs.submit({ op: "extract", archive: uri, dest: folder }) },
            { id: "extractTo", label: Kiki.T.tr("menu.extractTo"), enabled: !!row && row.kind === "archive", action: () => win.ops.extractTo(row.name, uri) },
            { id: "copyPath", label: Kiki.T.tr("menu.copyPath"), action: () => win.ops.copyPath(uris) },
            ...win.shareItems(uris),
            ...win.hereItems(row && row.isDir ? uri : folder, row && row.isDir ? [] : uris),
            { id: "trash", label: Kiki.T.tr("menu.trash"), key: "Del", danger: true, sep: true, action: () => win.ops.trashSelection(uris) },
        ]
    }
    /// The menu for the background of a folder — nothing under the pointer. The same list as a
    /// row's, so nothing moves about, with everything that needs a file already greyed out by
    /// `contextItems`; only the two actions that need somewhere to put things are re-aimed, since
    /// in columns view the folder clicked need not be the one the pane is on.
    function folderItems(folderUri) {
        win.pane.selection.clear()
        const items = contextItems(-1)
        // Named even when it IS the folder the pane is on: `New folder` untouched goes where the
        // KEY COLUMN is, which in columns is another folder entirely once one has been drilled
        // into — and the menu belongs to the column it was raised over, not to that one.
        if (folderUri) {
            for (const it of items) {
                if (it.id === "paste") it.action = () => win.ops.paste(folderUri)
                else if (it.id === "newFolder") it.action = () => win.ops.newFolder(folderUri)
            }
        }
        return items
    }
    function contextItems(index) {
        // A placeholder goes in at once so the submenu is never empty; the applications land a
        // moment later and replace it, even if the submenu is already showing.
        openWithSub = openWithItems()
        const chosen = win.selectedUris()
        if (chosen.length) loadOpenWith(chosen, list => { openWithSub = list; if (win.menu.visible) win.menu.refill("openWith", list) })
        const r = index >= 0 ? win.pane.listing.row(index) : null
        const sel = win.pane.selection.count() > 0
        if (win.pane.isTrash) {
            const info = r && win.trashInfo[r.name]
            return [
                { label: info ? Kiki.T.tr("menu.restoreTo", { path: Kiki.Format.display(info.path.replace(/\/[^/]*$/, "") || "/", win.home) }) : Kiki.T.tr("menu.restore"), key: "Enter", enabled: sel, action: () => win.restoreSelection() },
                { id: "copyPath", label: Kiki.T.tr("menu.copyPath"), enabled: sel && !!info, action: () => Quickshell.execDetached(["wl-copy", info.path]) },
                { id: "emptyTrash", label: Kiki.T.tr("menu.emptyTrash"), danger: true, sep: true, enabled: win.pane.listing.count > 0, action: () => win.emptyTrash() },
            ]
        }
        const items = [
            { id: "open", label: Kiki.T.tr("menu.open"), key: "Enter", enabled: sel, action: () => win.openSelected() },
            { id: "openWith", label: Kiki.T.tr("menu.openWith"), enabled: sel, items: openWithSub },
            { id: "getInfo", label: Kiki.T.tr("menu.getInfo"), key: "Ctrl+I", enabled: sel, action: () => win.inspectorRequested = true },
            { id: "copy", label: Kiki.T.tr("menu.copy"), key: "Super+C", sep: true, enabled: sel, action: () => win.copySelection(false) },
            { id: "cut", label: Kiki.T.tr("menu.cut"), key: "Super+X", enabled: sel, action: () => win.copySelection(true) },
            { id: "paste", label: Kiki.T.tr("menu.paste"), key: "Super+V", enabled: win.clipboard.uris.length > 0, action: () => win.paste() },
            { id: "newFolder", label: Kiki.T.tr("menu.newFolder"), key: "Ctrl+Shift+N", sep: true, action: () => win.newFolder() },
            { id: "rename", label: Kiki.T.tr("menu.rename"), key: "F2", enabled: sel && win.pane.selection.count() === 1, action: () => win.renameSelected() },
            { id: "compress", label: Kiki.T.tr("menu.compress"), enabled: sel, action: () => win.compressDialog.open(win.selectedUris(), win.pane.uri) },
            { id: "extractHere", label: Kiki.T.tr("menu.extractHere"), enabled: !!r && r.kind === "archive", action: () => win.ops.extractHere(r.name) },
            { id: "extractTo", label: Kiki.T.tr("menu.extractTo"), enabled: !!r && r.kind === "archive", action: () => win.ops.extractTo(r.name) },
            { id: "copyPath", label: Kiki.T.tr("menu.copyPath"), enabled: sel, action: () => win.copyPath() },
            ...win.shareItems(),
            ...win.hereItems(r && r.isDir && win.pane.selection.count() === 1 ? win.pane.childUri(r.name) : win.pane.uri, win.selectedUris().filter(u => !(r && r.isDir && win.pane.selection.count() === 1))),
            { id: "trash", label: Kiki.T.tr("menu.trash"), key: "Del", danger: true, sep: true, enabled: sel, action: () => win.trashSelection() },
        ]
        return items
    }

    /// The context menu for what is chosen now — the Menu key's list, and the one the IPC acts
    /// through. In columns the chosen row can live in a folder the pane is not standing in, so it
    /// is asked for by URI, exactly as the right click on that row is; with nothing highlighted
    /// there, the menu is the key column's folder's, as a right click on its empty space gives.
    function contextItemsNow() {
        const c = win.columnsPane()
        if (!c) return contextItems(win.pane.selection.current)
        const uris = c.selectedUris(), r = c.selectedRow()
        if (uris.length && r) return contextItemsForUri(uris[0], r)
        return folderItems(c.columns.length ? c.columns[c.focusCol].uri : win.pane.uri)
    }
}
