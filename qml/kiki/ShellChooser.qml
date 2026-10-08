import QtQuick
import "." as Kiki
import "ui" as UI

// The window's face to the desktop, split out of Shell.qml (docs/0.5.0/04-shell-split.md): the
// file chooser another application's Open or Save asks for through `kiki-dbus`, the answers
// kept by token until the listener collects them, and "Show in folder" (`showItems`).
//
// The chooser is no longer drawn here: it has a surface of its own on the overlay layer
// (`ui/ChooserWindow.qml`, docs/0.5.0/11-chooser-window.md), above the application that asked
// instead of inside this window and behind it. `raise` stays for "Show in folder", which really
// does want this window in front — a second `kiki` asks for the same.
Item {
    id: shellChooser
    anchors.fill: parent
    required property var win

    /// The chooser window, made the first time one is asked for: a session that never opens a
    /// chooser never builds its pane, its listing, its sidebar or its search.
    property Loader chooserLoader: Loader {
        active: false
        sourceComponent: UI.ChooserWindow {
            ghost: shellChooser.win.left.ghost
            home: shellChooser.win.home
            favorites: shellChooser.win.favorites
            locations: shellChooser.win.locations
            chooser: ({ answered: (token, uris) => shellChooser.chooserFinished(token, uris) })
            // The keys come back to the file manager's own handler — an item inside this window,
            // not a raise of it: the compositor gives the keyboard back to whoever had it, which
            // after another application's dialog is that application.
            onDismissed: shellChooser.win.keys.forceActiveFocus()
        }
    }
    /// The chooser, built if it is not there yet. Everything that opens one goes through here.
    function chooserWindow() { chooserLoader.active = true; return chooserLoader.item }
    /// Whether a chooser is up, for whoever asks (the IPC's `dialogs.portal`). Never builds it.
    readonly property bool chooserUp: chooserLoader.item ? chooserLoader.item.up : false
    /// The same chooser, asked by kiki itself rather than through the portal: `cb(uris)` gets
    /// the answer (null when cancelled) and nothing goes to the daemon.
    function pick(r, cb) { chooserWindow().pick(r, cb) }
    /// Answer the chooser that is up, if one is (the IPC's `chooser save <path>`).
    function finishChooser(uris) { if (chooserLoader.item) chooserLoader.item.dialog.finish(uris) }

    /// Answers for choosers that have finished, by token, until the listener collects them. A
    /// listener that dies before collecting leaves one entry; they are small and the window is
    /// not a server, so they are dropped after `chooserKeep`.
    property var chooserDone: ({})
    readonly property int chooserKeep: 5 * 60 * 1000
    function startChooser(r) {
        // No `raise`: the chooser is above every window already, and raising this one would pull
        // the person off whatever they were doing to the workspace the file manager is on.
        chooserWindow().show(r)
    }
    /// Called by the portal when a chooser the LISTENER asked for is answered (`uris` null when
    /// it was cancelled). A chooser kiki asked itself never comes here.
    function chooserFinished(token, uris) {
        if (!token) return
        const d = Object.assign({}, chooserDone)
        d[token] = { at: Date.now(), uris: uris || [], cancelled: !uris }
        chooserDone = d
    }
    /// What the listener collects: pending until the person has chosen, then the answer once.
    function chooserAnswer(token) {
        const got = chooserDone[token]
        if (!got) return { pending: true }
        const d = Object.assign({}, chooserDone)
        delete d[token]
        for (const t in d) if (Date.now() - d[t].at > chooserKeep) delete d[t]
        chooserDone = d
        return got.cancelled ? { cancelled: true } : { uris: got.uris }
    }
    /// org.freedesktop.FileManager1: ShowFolders, ShowItems, ShowItemProperties.
    function showItems(msg) {
        const uris = msg.uris || []; if (!uris.length) return
        const first = uris[0]
        win.raise()
        if (msg.folders) { win.pane.open(first); return }
        const parent = first.replace(/\/[^/]*$/, "") || first
        win.pane.open(parent)
        // Selected once the folder has listed, and found by the daemon: the file may be far past
        // the rows the window holds. (Set after `open`, which decides this for itself.)
        win.pane.selectAfterLoad = decodeURIComponent(first.split("/").pop())
        win.selectCameFrom()
        if (msg.properties) win.inspectorRequested = true
    }
}
