import QtQuick
import "." as Kiki

// The keyboard, split out of Shell.qml (docs/0.5.0/04-shell-split.md): the dispatcher behind the
// window's `Keys.onPressed`, the rebindable actions by id (`runAction`, what the keymap table
// and the IPC's `action` reach), and the bare Vim letters. The attachment stays where it was —
// the focus item in Shell.qml hands every event here — and `Keymap.qml` keeps the chords.
QtObject {
    id: shellKeys
    required property var win

    /// What a key pressed in the window does, in the order Shell.qml always asked it: the
    /// rebindable table first, then the bare Vim letters, then the keys that belong to the view.
    function handleKey(event) {
        const ctrl = event.modifiers & Qt.ControlModifier, shift = event.modifiers & Qt.ShiftModifier, alt = event.modifiers & Qt.AltModifier
        // The rebindable shortcuts come first, from the table the shortcuts window edits.
        // Whatever is left is contextual — the arrows and the Vim letters, Enter, Backspace —
        // and belongs to the view.
        const action = win.keymap.idFor(event.key, event.modifiers, event.text)
        if (action && runAction(action)) { event.accepted = true; return }
        // The letters are commands only bare: with Ctrl or Alt they are chords, and a chord
        // the table does not have means nothing.
        if (!ctrl && !alt && vimKey(event.key, shift)) { event.accepted = true; return }
        switch (event.key) {
        // Bare keys are free in the gallery: it has no list to move about in.
        case Qt.Key_1: if (win.galleryPane()) win.galleryPane().actual(); else return; break
        case Qt.Key_0: if (win.galleryPane()) win.galleryPane().fit(); else return; break
        case Qt.Key_Plus: case Qt.Key_Equal: if (win.galleryPane()) win.galleryPane().zoomBy(1.25); else return; break
        case Qt.Key_Minus: if (win.galleryPane()) win.galleryPane().zoomBy(0.8); else return; break
        // In the gallery Space steps, as it always has; elsewhere it is Quick Look, on and off.
        case Qt.Key_Space: if (win.galleryPane()) win.galleryPane().step(1); else if (win.quickLookWin.visible) win.quickLookWin.close(); else if (!win.openQuickLook()) return; break
        case Qt.Key_Down: win.keyDown(shift); break
        case Qt.Key_Up: if (alt) win.pane.up(); else win.keyUp(shift); break
        case Qt.Key_Home: win.selectAt(0, shift || win.visual); break
        case Qt.Key_End: win.selectAt(win.pane.listing.count - 1, shift || win.visual); break
        case Qt.Key_PageDown: win.moveSelection(win.pageStep, shift || win.visual); break
        case Qt.Key_PageUp: win.moveSelection(-win.pageStep, shift || win.visual); break
        case Qt.Key_Return: case Qt.Key_Enter: if (win.sidebarFocus) { win.sidebarPanel.activateKey(); win.focusSidebar(false); break } if (win.columnsPane()) { win.columnsPane().activateKey(); break } { const rr = win.pane.listing.row(win.pane.selection.current); if (rr && rr.isDir && !win.pane.isTrash) { win.openFolder(win.pane.childUri(rr.name)); break } } win.openSelected(); break
        case Qt.Key_Backspace: win.pane.up(); break
        // Left leaves a folder, Right enters one, whichever view is showing; with Alt they
        // walk the history. (In columns, Left first walks back through the columns the
        // inspector pushed off screen.)
        case Qt.Key_Left: if (alt) win.pane.back(); else win.keyLeft(); break
        case Qt.Key_Right: if (alt) win.pane.forward(); else win.keyRight(); break
        case Qt.Key_Menu: win.openMenuKey(); break
        // The mirror workspace answers Escape itself where it has something to stop — the
        // compare on its Preflight screen — and leaves it alone on its other screens.
        case Qt.Key_Escape: if (win.visual) win.visual = false; else if (win.activity.visible) win.activity.close(); else if (win.infoPopover.visible) win.inspectorRequested = false; else if (win.mirrorOpen && win.mirrorWs.escapeKey()) { /* the workspace stopped its compare */ } else if (win.sidebarFocus) win.focusSidebar(false); else if (win.galleryPane()) win.pane.view = win.galleryFrom; else win.pane.selection.clear(); break
        case Qt.Key_Tab: if (win.split) win.focusPane(win.otherPane()); else return; break
        default: return
        }
        event.accepted = true
    }

    /// Run a rebindable action by id. Returns false when the action is not available now, so the
    /// keypress can fall through to whatever the view makes of it.
    function runAction(id) {
        switch (id) {
        case "filter": case "filterAlt": win.openFilter(); return true
        case "search": win.openSearch(win.leftFilter.text); return true
        case "typePath": win.activeCrumb().edit(); return true
        case "addLocation": win.locationDialog.open(null); return true
        case "viewIcon": win.setView("icon"); return true
        case "viewList": win.setView("list"); return true
        case "viewColumns": win.setView("columns"); return true
        case "viewMirror": win.toggleMirrorView(); return true
        case "viewGallery": win.enterGallery(); return true
        case "hidden": win.pane.setHidden(!win.pane.showHidden); return true
        case "inspector": win.inspectorRequested = !win.inspectorRequested; return true
        case "refresh": win.pane.listing.refresh(); return true
        case "sidebar": win.sidebarShown = !win.sidebarShown; return true
        case "focusSidebar": win.focusSidebar(!win.sidebarFocus); return true
        case "settings": win.settingsWin.open("general"); return true
        case "shortcuts": win.keysWin.open(); return true
        case "copy": win.copySelection(false); return true
        case "cut": win.copySelection(true); return true
        case "paste": win.paste(); return true
        case "copyPath": win.copyPath(); return true
        case "newFolder": win.newFolder(); return true
        case "rename": win.renameSelected(); return true
        case "edit": win.editSelected(); return true
        case "trash": if (win.pane.isTrash) win.deleteForever(); else win.trashSelection(); return true
        case "deleteForever": win.deleteForever(); return true
        case "undo": Kiki.Jobs.undo(); return true
        case "redo": Kiki.Jobs.redo(); return true
        case "selectAll": for (let i = 0; i < win.pane.listing.count; i++) win.pane.selection.rows[i] = true; win.pane.selection.changed(); return true
        case "openWith": win.openWithMenu(); return true
        case "openDefault": win.openIn(""); return true
        case "share": win.shareMenu(); return true
        case "ai": win.openAiHere(win.pane.uri, win.selectedUris()); return true
        case "mirror": win.toggleMirror(); return true
        case "transfer": if (!win.split) return false; win.transfer(true); return true
        case "project": if (win.projectMode) win.leaveProject(); else { const u = win.selectedUris(), r = win.selectedRow(); win.enterProject(u.length && r && r.isDir ? u[0] : win.pane.uri) } return true
        case "terminal": if (win.pane.uri.indexOf("file://") !== 0) return false; win.openTerminalHere(win.pane.uri); return true
        case "eject": { const d = win.devices.find(d => win.pane.uri.startsWith(d.uri.replace(/\/$/, ""))); if (!d) return false; Kiki.Daemon.request("Eject", { uri: d.uri }); return true }
        }
        return false
    }

    /// `dd` trashes: the first d waits 600 ms for the second.
    property bool pendingD: false
    property Timer ddTimer: Timer { interval: 600; onTriggered: shellKeys.pendingD = false }
    /// The Vim letters, bare (no Ctrl, no Alt): whether `key` was one of them. In the gallery
    /// its own bare keys keep their meaning (f is the filmstrip there).
    function vimKey(key, shift) {
        if (key !== Qt.Key_D) pendingD = false
        switch (key) {
        case Qt.Key_J: win.keyDown(shift); return true
        case Qt.Key_K: win.keyUp(shift); return true
        case Qt.Key_H: win.keyLeft(); return true
        case Qt.Key_L: win.keyRight(); return true
        case Qt.Key_V: win.visual = !win.visual; if (win.visual && win.pane.selection.current >= 0 && !Object.keys(win.pane.selection.rows).length) win.pane.selection.set(win.pane.selection.current); return true
        case Qt.Key_Y: win.visual = false; return runAction("copy")
        case Qt.Key_X: win.visual = false; return runAction("cut")
        case Qt.Key_P: return runAction("paste")
        case Qt.Key_R: return runAction("rename")
        case Qt.Key_Z: return runAction(shift ? "redo" : "undo")
        case Qt.Key_E: win.editSelected(); return true
        case Qt.Key_I: win.inspectorRequested = !win.inspectorRequested; return true
        case Qt.Key_F: if (win.galleryPane()) win.galleryPane().filmstrip = !win.galleryPane().filmstrip; else win.openFilter(); return true
        case Qt.Key_Colon: return runAction("typePath")
        case Qt.Key_M: win.openMenuKey(); return true
        case Qt.Key_Period: return runAction("hidden")
        case Qt.Key_D: if (pendingD) { pendingD = false; win.visual = false; return runAction("trash") } pendingD = true; ddTimer.restart(); return true
        }
        return false
    }
}
