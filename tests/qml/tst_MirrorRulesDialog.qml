import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// Edit rules… (plan 08): the list of what a mirror never transfers. What is checked here is the
// list itself — what the daemon's answer puts on screen, what each control does to it, and what
// Done sends back — because a rule saved wrong is a file that silently stops being mirrored.
TestCase {
    id: tc
    name: "MirrorRulesDialog"
    when: windowShown
    visible: true
    width: 700; height: 520

    UI.MirrorRulesDialog { id: dlg; anchors.fill: parent }

    /// The built-in rules: nine names, not a pattern. A dotted name is mirrored unless it is one
    /// of these, because `.htaccess` and `.well-known/` are what a website mirror is for.
    readonly property var builtIn: [
        { kind: "matches", value: ".git" }, { kind: "matches", value: ".gitignore" }, { kind: "matches", value: ".DS_Store" }, { kind: "matches", value: ".env" },
        { kind: "matches", value: ".idea" }, { kind: "matches", value: ".vscode" }, { kind: "matches", value: "Thumbs.db" },
        { kind: "matches", value: "node_modules" }, { kind: "matches", value: "__pycache__" }]
    readonly property var builtInNames: [".git", ".gitignore", ".DS_Store", ".env", ".idea", ".vscode", "Thumbs.db", "node_modules", "__pycache__"]
    /// The daemon's answer with no `filters.toml` in the way.
    readonly property var defaultReply: ({ rules: tc.builtIn, defaults: true, defaultRules: tc.builtIn })
    /// An answer for rules of the user's own. Every reply carries `defaultRules`, in use or not:
    /// the daemon is the one authority for what the defaults are, and the dialog keeps no copy.
    function mine(rules) { return { rules: rules, defaults: false, defaultRules: tc.builtIn } }

    function init() {
        Wire.reset()
        dlg.visible = false
    }
    function cleanup() { dlg.visible = false }

    /// Open it and answer the read with `reply`; leaves the rows on screen.
    function opened(reply) {
        dlg.open()
        const asked = Wire.replyTo("MirrorFilters", reply || tc.defaultReply)
        verify(asked, "the dialog asks the daemon for the rules")
        return dlg.info()
    }
    function kinds() { return dlg.info().rules.map(r => r.kind) }
    function values() { return dlg.info().rules.map(r => r.value) }

    // ---------------------------------------------------------------- what is on screen
    function test_the_rows_are_the_rules_the_daemon_answered() {
        const st = opened()
        compare(st.open, true)
        compare(values(), tc.builtInNames)
        verify(kinds().every(k => k === "matches"), "nine names, every one of them — not a pattern: " + kinds())
        compare(st.defaults, true)
        compare(st.error, "")
        // Nothing is written by looking.
        compare(Wire.count("SetMirrorFilters"), 0)
    }

    // `starts with .` is the hidden files, and says so where it is: three characters that mean a
    // great deal more than they look like. It is not built in — a dotted name is mirrored unless
    // the rules name it — so this is the rule a user adds when they do want the lot skipped.
    function test_the_hidden_files_rule_says_what_it_is() {
        opened()
        for (let r = 0; r < 8; r++) compare(findChild(dlg, "rule-hint-" + r).visible, false, "no built-in rule claims to be the hidden files")
        dlg.addRule()
        dlg.setValue(8, ".")
        compare(findChild(dlg, "rule-hint-8").visible, false, "`is .` is a file called `.`, not the hidden ones")
        dlg.cycleKind(8); dlg.cycleKind(8)
        compare(kinds()[8], "startsWith")
        const hint = findChild(dlg, "rule-hint-8")
        verify(hint, "the row the user added has a hint")
        compare(hint.text, "hidden files")
        compare(hint.visible, true)
        // It is the rule that earns the hint, not the row: change either half and it goes.
        dlg.setValue(8, ".cache")
        compare(findChild(dlg, "rule-hint-8").visible, false)
        dlg.setValue(8, ".")
        compare(findChild(dlg, "rule-hint-8").visible, true)
        dlg.cycleKind(8)
        compare(findChild(dlg, "rule-hint-8").visible, false)
    }

    function test_the_explanation_says_what_a_rule_matches() {
        opened()
        const t = findChild(dlg, "mirror-rules-explanation").text
        verify(t.indexOf("NAME") >= 0 && t.indexOf("anywhere in the tree") >= 0, t)
        verify(t.indexOf("skipped with everything in it") >= 0, t)
        verify(t.indexOf("never deleted on the destination") >= 0, t)
    }

    // ---------------------------------------------------------------- the controls
    function test_the_kind_box_cycles_through_the_four() {
        opened(tc.mine([{ kind: "matches", value: ".git" }]))
        compare(kinds(), ["matches"])
        dlg.cycleKind(0); compare(kinds(), ["contains"])
        dlg.cycleKind(0); compare(kinds(), ["startsWith"])
        dlg.cycleKind(0); compare(kinds(), ["endsWith"])
        dlg.cycleKind(0); compare(kinds(), ["matches"], "and round again")
        compare(values(), [".git"], "cycling the kind does not touch the name")
    }

    function test_add_gives_an_empty_is_row_and_remove_takes_one_away() {
        opened(tc.mine([{ kind: "startsWith", value: "~" }, { kind: "matches", value: "node_modules" }, { kind: "matches", value: "__pycache__" }]))
        dlg.addRule()
        compare(kinds(), ["startsWith", "matches", "matches", "matches"])
        compare(values(), ["~", "node_modules", "__pycache__", ""])
        dlg.removeRule(1)
        compare(values(), ["~", "__pycache__", ""], "the row that was clicked, not the one after it")
        dlg.removeRule(2)
        compare(values(), ["~", "__pycache__"])
    }

    // Past the ninth rule the list scrolls, and the row Add rule just made has to be the one in
    // view: otherwise the cursor is in a box below the bottom of the dialog.
    function test_the_list_scrolls_and_a_new_rule_is_brought_into_view() {
        const many = []
        for (let i = 0; i < 12; i++) many.push({ kind: "matches", value: "rule" + i })
        opened(tc.mine(many))
        const flick = findChild(dlg, "mirror-rules-list")
        tryVerify(() => flick.contentHeight > flick.height, 2000, "twelve rules do not fit")
        dlg.addRule()
        tryVerify(() => flick.contentY === flick.contentHeight - flick.height, 2000, "the list is scrolled to the end")
        const box = findChild(dlg, "rule-value-12")
        verify(box && box.activeFocus, "and the cursor is in the row that was just made")
    }

    // The built-in nine fill the list exactly, so the tenth rule is the first one that has to be
    // scrolled to — and the commonest way to reach it is + Add rule on the defaults.
    function test_a_tenth_rule_on_the_built_in_nine_is_in_view() {
        opened()
        const flick = findChild(dlg, "mirror-rules-list")
        tryVerify(() => flick.contentHeight === flick.height, 2000, "the nine defaults fit without scrolling")
        dlg.addRule()
        tryVerify(() => flick.contentY === flick.contentHeight - flick.height, 2000, "the tenth scrolls the list to the end")
        const box = findChild(dlg, "rule-value-9")
        tryVerify(() => box && box.activeFocus, 2000, "and the cursor is in it")
        const at = box.mapToItem(flick, 0, 0)
        verify(at.y >= -0.5 && at.y + box.height <= flick.height + 0.5, "the new row is inside the visible part of the list, at " + at.y)
    }

    // A row takes the name of the one below it when the row above is removed. The box it is typed
    // in keeps its own text once it has been typed in, so this is the row that used to lie.
    function test_a_row_that_moves_up_shows_its_own_name() {
        opened(tc.mine([{ kind: "matches", value: "one" }, { kind: "matches", value: "two" }]))
        // Typed into, both of them, which is what puts the boxes on their own.
        findChild(dlg, "rule-value-0").text = "one edited"
        findChild(dlg, "rule-value-1").text = "two edited"
        compare(values(), ["one edited", "two edited"])
        dlg.removeRule(0)
        compare(values(), ["two edited"])
        compare(findChild(dlg, "rule-value-0").text, "two edited", "the box shows the rule it now holds")
    }

    function test_editing_a_name_is_what_done_sends() {
        opened(tc.mine([{ kind: "endsWith", value: ".bak" }]))
        dlg.setValue(0, ".tmp")
        dlg.addRule()
        dlg.setValue(1, "vendor")
        dlg.save()
        const sent = Wire.last("SetMirrorFilters")
        verify(sent, "Done writes")
        compare(sent.rules, [{ kind: "endsWith", value: ".tmp" }, { kind: "matches", value: "vendor" }])
        verify(sent.defaults === undefined, "an ordinary save is not a restore")
        // Still up until the daemon says it took them, then gone.
        compare(dlg.visible, true)
        Wire.replyTo("SetMirrorFilters", tc.mine(sent.rules))
        compare(dlg.visible, false)
    }

    // An Add rule nobody filled in is not a mistake worth a message; it is simply not a rule.
    function test_a_row_left_empty_is_dropped_rather_than_saved() {
        opened(tc.mine([{ kind: "matches", value: ".git" }]))
        dlg.addRule()
        dlg.addRule()
        dlg.setValue(2, "   ")
        dlg.save()
        compare(Wire.last("SetMirrorFilters").rules, [{ kind: "matches", value: ".git" }])
    }

    // A rule matches one NAME, so a value with a path separator in it could never match anything.
    // Marked where it is, and Done does not write until it is fixed.
    function test_a_name_with_a_slash_in_it_refuses_done() {
        opened(tc.mine([{ kind: "matches", value: ".git" }]))
        dlg.addRule()
        dlg.setValue(1, "build/out")
        dlg.save()
        compare(Wire.count("SetMirrorFilters"), 0, "nothing was written")
        compare(dlg.visible, true)
        compare(dlg.info().bad, [1], "and the row that is wrong is the one marked")
        verify(dlg.info().error.indexOf("not a path") >= 0, dlg.info().error)
        // Fixing it clears the mark and the message, and then it saves.
        dlg.setValue(1, "out")
        compare(dlg.info().bad, [])
        compare(dlg.info().error, "")
        dlg.save()
        compare(Wire.last("SetMirrorFilters").rules, [{ kind: "matches", value: ".git" }, { kind: "matches", value: "out" }])
    }

    // What the daemon refuses is said in the daemon's words rather than swallowed.
    function test_a_refusal_from_the_daemon_is_shown() {
        opened(tc.mine([{ kind: "matches", value: ".git" }]))
        dlg.setValue(0, "x")
        dlg.save()
        const req = Wire.last("SetMirrorFilters")
        Wire.fail(req.id, "Invalid", "rule 1: value must not be empty")
        compare(dlg.visible, true, "it stays up so the rule can be fixed")
        compare(dlg.info().error, "rule 1: value must not be empty")
    }

    // Restore defaults is the file going away, not a list of rules kept in here. What it shows is
    // what the daemon's reply said the defaults are — proved with a reply whose defaults are
    // nothing like the built-in set: the dialog has to show THOSE, because it has no other list.
    function test_restore_defaults_shows_the_defaults_the_daemon_sent() {
        const theirs = [{ kind: "matches", value: "cellar" }, { kind: "endsWith", value: ".sqlite" }]
        opened({ rules: [{ kind: "endsWith", value: ".bak" }], defaults: false, defaultRules: theirs })
        dlg.restoreDefaults()
        compare(kinds(), ["matches", "endsWith"])
        compare(values(), ["cellar", ".sqlite"], "the daemon's list, not one written into the dialog")
        dlg.save()
        const sent = Wire.last("SetMirrorFilters")
        compare(sent.defaults, true)
        verify(sent.rules === undefined, "restoring removes the file rather than writing rules")
        Wire.replyTo("SetMirrorFilters", tc.defaultReply)
        compare(dlg.info().defaults, true)
        compare(values(), tc.builtInNames, "and the answer is what fills the rows")
        compare(dlg.visible, false)
    }

    // A dialog that has never had a reply has nothing to preview — it cannot happen through the
    // button, which opens on one — and Done still takes the file away.
    function test_restoring_before_any_reply_shows_nothing_and_still_restores() {
        dlg.defaultRules = []
        dlg.rows = [{ kind: "matches", value: "whatever" }]
        dlg.restoreDefaults()
        compare(values(), [])
        dlg.save()
        compare(Wire.last("SetMirrorFilters").defaults, true)
    }

    // Touched after restoring, it is an edit of the defaults like any other and saves as rules.
    function test_an_edit_after_restoring_saves_as_rules() {
        opened(tc.mine([{ kind: "endsWith", value: ".bak" }]))
        dlg.restoreDefaults()
        compare(values(), tc.builtInNames)
        dlg.removeRule(8)
        dlg.save()
        const sent = Wire.last("SetMirrorFilters")
        verify(sent.defaults === undefined, JSON.stringify(sent))
        compare(sent.rules.map(r => r.value), tc.builtInNames.slice(0, 8))
    }

    // Every rule deleted is a real answer — transfer everything — and it has to reach the daemon
    // as one, or an empty file would read back as the defaults on the next scan.
    function test_deleting_every_rule_is_sent_as_no_rules() {
        opened()
        while (dlg.info().rules.length) dlg.removeRule(0)
        compare(values(), [])
        verify(findChild(dlg, "mirror-rules-empty").visible, "and it says so on screen")
        dlg.save()
        const sent = Wire.last("SetMirrorFilters")
        compare(sent.rules, [])
        verify(sent.defaults === undefined)
    }

    // ---------------------------------------------------------------- cancel
    function test_cancel_writes_nothing_and_escape_is_cancel() {
        opened()
        dlg.setValue(0, "..")
        dlg.addRule()
        dlg.cancel()
        compare(dlg.visible, false)
        compare(Wire.count("SetMirrorFilters"), 0)

        opened()
        keyClick(Qt.Key_Escape)
        compare(dlg.visible, false)
        compare(Wire.count("SetMirrorFilters"), 0)
    }

    // The workspace only goes back to Configure when the rules really changed: the plan it is
    // showing was made with them, and re-scanning for a Done that changed nothing is noise.
    function test_saved_is_only_announced_when_something_changed() {
        let announced = 0
        const bump = function () { announced++ }
        dlg.saved.connect(bump)

        opened()
        dlg.save()
        Wire.replyTo("SetMirrorFilters", tc.mine(tc.defaultReply.rules))
        compare(announced, 0, "the same eight rules, now written down: nothing to re-scan for")

        opened(tc.mine([{ kind: "matches", value: ".git" }]))
        dlg.setValue(0, ".svn")
        dlg.save()
        Wire.replyTo("SetMirrorFilters", tc.mine([{ kind: "matches", value: ".svn" }]))
        compare(announced, 1)
        dlg.saved.disconnect(bump)
    }
}
