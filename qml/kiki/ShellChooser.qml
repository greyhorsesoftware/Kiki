import QtQuick
import "." as Kiki
import "ui" as UI

// The window's face to the desktop, split out of Shell.qml (docs/0.5.0/04-shell-split.md): the
// file chooser another application's Open or Save asks for through `kiki-dbus`, the answers
// kept by token until the listener collects them, and "Show in folder" (`showItems`). The
// chooser dialog itself lives here too, filling the window as it did from Shell.qml; `raise`
// stays the window's, since a second `kiki` asks for it as well.
Item {
    id: shellChooser
    anchors.fill: parent
    required property var win
    /// The chooser dialog, for whoever asks whether it is up.
    property alias portal: portal

    /// Answers for choosers that have finished, by token, until the listener collects them. A
    /// listener that dies before collecting leaves one entry; they are small and the window is
    /// not a server, so they are dropped after `chooserKeep`.
    property var chooserDone: ({})
    readonly property int chooserKeep: 5 * 60 * 1000
    function startChooser(r) {
        win.raise()
        portal.open(r)
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
    UI.PortalDialog { id: portal; objectName: "portal"; ghost: win.left.ghost; chooser: ({ answered: (token, uris) => chooserFinished(token, uris) }); anchors.fill: parent; home: win.home; favorites: win.favorites; locations: win.locations }
}
