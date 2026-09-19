import QtQuick
import "." as Kiki

// The keymap as data: every shortcut a person can change, what it is called, and what it is
// bound to. `Shell.qml` asks this what a keypress means before it reaches its own handler, and
// the shortcuts window edits the same table, so the two can never disagree.
//
// Contextual keys — the arrows, Enter, Backspace, Vim keys, type-ahead — are not here: they
// mean different things in each view and there is nothing useful to rebind them to.
QtObject {
    id: keymap

    readonly property var actions: [
        // Moving around
        { id: "filter",        group: "Find",     label: "Filter this folder",     def: "/" },
        { id: "filterAlt",     group: "Find",     label: "Filter (alternative)",   def: "Ctrl+F" },
        { id: "search",        group: "Find",     label: "Search everywhere",      def: "Ctrl+Shift+F" },
        { id: "typePath",      group: "Find",     label: "Type a path",            def: "Ctrl+L" },
        { id: "addLocation",   group: "Find",     label: "Add a location",         def: "Ctrl+Shift+L" },

        { id: "viewIcon",      group: "View",     label: "Icon view",              def: "Ctrl+1" },
        { id: "viewList",      group: "View",     label: "List view",              def: "Ctrl+2" },
        { id: "viewColumns",   group: "View",     label: "Columns view",           def: "Ctrl+3" },
        { id: "viewMirror",    group: "View",     label: "Side by Side",           def: "Ctrl+4" },
        { id: "viewGallery",   group: "View",     label: "Gallery view",           def: "Ctrl+5" },
        { id: "hidden",        group: "View",     label: "Show hidden files",      def: "Ctrl+H" },
        { id: "inspector",     group: "View",     label: "Info panel",             def: "Ctrl+I" },
        { id: "refresh",       group: "View",     label: "Refresh",                def: "F5" },
        { id: "sidebar",       group: "View",     label: "Show or hide favorites", def: "Ctrl+Shift+B" },
        { id: "focusSidebar",  group: "View",     label: "Focus favorites",        def: "Ctrl+B" },
        { id: "settings",      group: "View",     label: "Settings",               def: "Ctrl+," },
        { id: "shortcuts",     group: "View",     label: "Keyboard shortcuts",     def: "Ctrl+?" },

        { id: "copy",          group: "Files",    label: "Copy",                   def: "Super+C" },
        { id: "cut",           group: "Files",    label: "Cut",                    def: "Super+X" },
        { id: "paste",         group: "Files",    label: "Paste",                  def: "Super+V" },
        { id: "copyPath",      group: "Files",    label: "Copy path",              def: "Super+Shift+C" },
        { id: "newFolder",     group: "Files",    label: "New folder",             def: "Ctrl+Shift+N" },
        { id: "rename",        group: "Files",    label: "Rename",                 def: "F2" },
        { id: "edit",          group: "Files",    label: "Edit",                   def: "F4" },
        { id: "trash",         group: "Files",    label: "Move to trash",          def: "Del" },
        { id: "deleteForever", group: "Files",    label: "Delete for good",        def: "Shift+Del" },
        { id: "undo",          group: "Files",    label: "Undo",                   def: "Ctrl+Z" },
        { id: "redo",          group: "Files",    label: "Redo",                   def: "Ctrl+Shift+Z" },
        { id: "selectAll",     group: "Files",    label: "Select all",             def: "Ctrl+A" },
        { id: "openWith",      group: "Files",    label: "Open with…",             def: "Alt+Shift+Enter" },
        { id: "openDefault",   group: "Files",    label: "Open in default tool",   def: "Alt+Enter" },
        { id: "share",         group: "Files",    label: "Share",                  def: "Alt+S" },
        { id: "ai",            group: "Files",    label: "Ask Jarvis",             def: "Alt+Q" },

        { id: "mirror",        group: "Two panes", label: "Mirror to the remote",  def: "Ctrl+M" },
        { id: "transfer",      group: "Two panes", label: "Move across",           def: "F6" },
        { id: "project",       group: "Two panes", label: "Project mode",          def: "Ctrl+Shift+P" },
        { id: "eject",         group: "Two panes", label: "Eject this device",     def: "Ctrl+E" },
    ]

    /// id -> chord, for the ones that have been changed. Lives in settings.toml under [keys].
    readonly property var custom: Kiki.Settings.keys || ({})

    readonly property var groups: {
        const out = []
        for (const a of actions) if (out.indexOf(a.group) < 0) out.push(a.group)
        return out
    }

    function find(id) { return actions.find(a => a.id === id) || null }
    function chordFor(id) { const c = custom[id]; if (c !== undefined) return c; const a = find(id); return a ? a.def : "" }
    function isCustom(id) { const a = find(id); return !!a && custom[id] !== undefined && custom[id] !== a.def }

    /// Which action a chord is already on, so the editor can warn rather than shadow.
    function idForChord(chord, except) {
        for (const a of actions) if (a.id !== except && chordFor(a.id) === chord) return a.id
        return ""
    }

    function rebind(id, chord) { Kiki.Settings.set("keys", id, chord) }
    function reset(id) { const a = find(id); if (a) Kiki.Settings.set("keys", id, a.def) }
    function resetAll() { for (const a of actions) if (isCustom(a.id)) Kiki.Settings.set("keys", a.id, a.def) }

    // ---------------------------------------------------------------- reading a key event

    /// Special keys by their Qt code; anything else is its own character.
    readonly property var names: ({
        0x01000000: "Esc", 0x01000001: "Tab", 0x01000003: "Backspace", 0x01000004: "Enter", 0x01000005: "Enter",
        0x01000007: "Del", 0x01000010: "Home", 0x01000011: "End", 0x01000012: "Left", 0x01000013: "Up",
        0x01000014: "Right", 0x01000015: "Down", 0x01000016: "PgUp", 0x01000017: "PgDn", 0x20: "Space",
        0x01000030: "F1", 0x01000031: "F2", 0x01000032: "F3", 0x01000033: "F4", 0x01000034: "F5",
        0x01000035: "F6", 0x01000036: "F7", 0x01000037: "F8", 0x01000038: "F9", 0x01000039: "F10",
        0x0100003a: "F11", 0x0100003b: "F12"
    })

    /// A keypress as the chord string the table uses: "Ctrl+Shift+N", "Super+C", "F2", "?".
    function encode(key, modifiers, text) {
        // Digits and letters only: the codes between them are punctuation, whose character
        // already carries the Shift (Shift+/ is "?", and the chord is Ctrl+? rather than
        // Ctrl+Shift+?, which is how every other application writes it).
        const plain = (key >= 0x30 && key <= 0x39) || (key >= 0x41 && key <= 0x5a)
        let name = names[key]
        if (name === undefined) {
            if (plain) name = String.fromCharCode(key)
            else if (text && text.length === 1 && text.charCodeAt(0) > 32) name = text.toUpperCase()
            else return ""
        }
        // Shift is part of the chord only where it does not already change the character.
        const named = names[key] !== undefined || plain
        let out = ""
        if (modifiers & Qt.ControlModifier) out += "Ctrl+"
        if (modifiers & Qt.MetaModifier) out += "Super+"
        if (modifiers & Qt.AltModifier) out += "Alt+"
        if ((modifiers & Qt.ShiftModifier) && named) out += "Shift+"
        return out + name
    }

    /// The action a keypress means, or "" when it means nothing here.
    function idFor(key, modifiers, text) {
        const chord = encode(key, modifiers, text)
        if (!chord) return ""
        for (const a of actions) if (chordFor(a.id) === chord) return a.id
        // Omarchy rewrites Super+C/X/V to Ctrl+C/X/V before a window sees them, so a binding on
        // one of those answers to the other as well.
        if (chord.indexOf("Ctrl+") === 0) {
            const asSuper = "Super+" + chord.slice(5)
            for (const a of actions) if (chordFor(a.id) === asSuper) return a.id
        }
        return ""
    }
}
