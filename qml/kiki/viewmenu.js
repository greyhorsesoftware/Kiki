.pragma library

// The view menu's rows, without what they do: which views there are, their keys, and which one is
// ticked. `view` is the FOCUSED pane's — side by side each pane has its own, and the tick follows
// the focus. Kept apart from Shell.qml so that it can be tested; the shell attaches the actions.
function items(view, showHidden) {
    return [
        { id: "icon", label: "Icon", key: "Ctrl+1", checked: view === "icon" },
        { id: "list", label: "List", key: "Ctrl+2", checked: view === "list" },
        { id: "columns", label: "Columns", key: "Ctrl+3", checked: view === "columns" },
        { id: "gallery", label: "Gallery", key: "Ctrl+5", checked: view === "gallery" },
        { id: "hidden", label: "Show hidden files", key: "Ctrl+H", sep: true, checked: showHidden === true },
    ]
}
