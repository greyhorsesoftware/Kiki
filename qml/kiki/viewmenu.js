.pragma library

// The view menu's rows, without what they do: which views there are, their keys, and which one is
// ticked. `view` is the FOCUSED pane's — side by side each pane has its own, and the tick follows
// the focus. Kept apart from Shell.qml so that it can be tested; the shell attaches the actions.
// `tr` is `Kiki.T.tr` — a library cannot reach the singleton itself. `local` is whether the pane's
// folder is on this machine: the gallery is greyed out on a server (owner, 2026-09-25: "disallow
// gallery view for remote sftp/ftps views, just dim it in menu") — every picture would be a
// fetch, and Quick Look is the way to look at one.
function items(view, showHidden, tr, local) {
    return [
        { id: "icon", label: tr("view.icon"), key: "Ctrl+1", checked: view === "icon" },
        { id: "list", label: tr("view.list"), key: "Ctrl+2", checked: view === "list" },
        { id: "columns", label: tr("view.columns"), key: "Ctrl+3", checked: view === "columns" },
        { id: "gallery", label: tr("view.gallery"), key: "Ctrl+5", checked: view === "gallery", enabled: local !== false },
        { id: "hidden", label: tr("view.hidden"), key: ".", sep: true, checked: showHidden === true },
    ]
}
