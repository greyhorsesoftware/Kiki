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
        { id: "filter",        group: "find",     label: Kiki.T.tr("action.filter"),     def: "/" },
        { id: "filterAlt",     group: "find",     label: Kiki.T.tr("action.filterAlt"),   def: "Ctrl+F" },
        { id: "search",        group: "find",     label: Kiki.T.tr("action.search"),      def: "Ctrl+Shift+F" },
        { id: "typePath",      group: "find",     label: Kiki.T.tr("action.typePath"),            def: "Ctrl+L" },
        { id: "addLocation",   group: "find",     label: Kiki.T.tr("action.addLocation"),         def: "Ctrl+Shift+L" },

        { id: "viewIcon",      group: "view",     label: Kiki.T.tr("action.viewIcon"),              def: "Ctrl+1" },
        { id: "viewList",      group: "view",     label: Kiki.T.tr("action.viewList"),              def: "Ctrl+2" },
        { id: "viewColumns",   group: "view",     label: Kiki.T.tr("action.viewColumns"),           def: "Ctrl+3" },
        { id: "viewMirror",    group: "view",     label: Kiki.T.tr("action.viewMirror"),           def: "Ctrl+4" },
        { id: "viewGallery",   group: "view",     label: Kiki.T.tr("action.viewGallery"),           def: "Ctrl+5" },
        { id: "hidden",        group: "view",     label: Kiki.T.tr("action.hidden"),      def: "Ctrl+H" },
        { id: "inspector",     group: "view",     label: Kiki.T.tr("action.inspector"),             def: "Ctrl+I" },
        { id: "refresh",       group: "view",     label: Kiki.T.tr("action.refresh"),                def: "F5" },
        { id: "sidebar",       group: "view",     label: Kiki.T.tr("action.sidebar"), def: "Ctrl+Shift+B" },
        { id: "focusSidebar",  group: "view",     label: Kiki.T.tr("action.focusSidebar"),        def: "Ctrl+B" },
        { id: "settings",      group: "view",     label: Kiki.T.tr("action.settings"),               def: "Ctrl+," },
        { id: "shortcuts",     group: "view",     label: Kiki.T.tr("action.shortcuts"),     def: "Ctrl+?" },

        { id: "copy",          group: "files",    label: Kiki.T.tr("action.copy"),                   def: "Super+C" },
        { id: "cut",           group: "files",    label: Kiki.T.tr("action.cut"),                    def: "Super+X" },
        { id: "paste",         group: "files",    label: Kiki.T.tr("action.paste"),                  def: "Super+V" },
        { id: "copyPath",      group: "files",    label: Kiki.T.tr("action.copyPath"),              def: "Super+Shift+C" },
        { id: "newFolder",     group: "files",    label: Kiki.T.tr("action.newFolder"),             def: "Ctrl+Shift+N" },
        { id: "rename",        group: "files",    label: Kiki.T.tr("action.rename"),                 def: "F2" },
        { id: "edit",          group: "files",    label: Kiki.T.tr("action.edit"),                   def: "F4" },
        { id: "trash",         group: "files",    label: Kiki.T.tr("action.trash"),          def: "Del" },
        { id: "deleteForever", group: "files",    label: Kiki.T.tr("action.deleteForever"),        def: "Shift+Del" },
        { id: "undo",          group: "files",    label: Kiki.T.tr("action.undo"),                   def: "Ctrl+Z" },
        { id: "redo",          group: "files",    label: Kiki.T.tr("action.redo"),                   def: "Ctrl+Shift+Z" },
        { id: "selectAll",     group: "files",    label: Kiki.T.tr("action.selectAll"),             def: "Ctrl+A" },
        { id: "openWith",      group: "files",    label: Kiki.T.tr("action.openWith"),             def: "Alt+Shift+Enter" },
        { id: "openDefault",   group: "files",    label: Kiki.T.tr("action.openDefault"),   def: "Alt+Enter" },
        { id: "share",         group: "files",    label: Kiki.T.tr("action.share"),                  def: "Alt+S" },
        { id: "ai",            group: "files",    label: Kiki.T.tr("action.ai"),            def: "Alt+Q" },

        { id: "mirror",        group: "twoPanes", label: Kiki.T.tr("action.mirror"),  def: "Ctrl+M" },
        { id: "transfer",      group: "twoPanes", label: Kiki.T.tr("action.transfer"),           def: "F6" },
        { id: "project",       group: "twoPanes", label: Kiki.T.tr("action.project"),          def: "Ctrl+Shift+P" },
        { id: "eject",         group: "twoPanes", label: Kiki.T.tr("action.eject"),     def: "Ctrl+E" },
    ]

    /// id -> chord, for the ones that have been changed. Lives in settings.toml under [keys].
    readonly property var custom: Kiki.Settings.keys || ({})

    /// A group's words, for the shortcuts window.
    function groupLabel(id) { return Kiki.T.tr("group." + id) }
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
