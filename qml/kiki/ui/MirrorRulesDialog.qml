import QtQuick
import ".." as Kiki

// Edit rules… (plan 08): the mirror's filter rules, which are the list of what NOT to transfer.
// One row per rule — the kind as a box that cycles, the name to match, and an x — over the
// Configure screen. The rules live in the daemon's `filters.toml`; this reads them with
// `MirrorFilters` and writes them back with `SetMirrorFilters`, and nothing is written until Done.
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 30

    /// The rows being edited: { kind, value }. Not the saved rules — those are the daemon's, and
    /// they are only replaced when a save comes back.
    property var rows: []
    /// Whether what is saved is still the built-in set, as the daemon last said.
    property bool defaults: true
    /// The built-in set itself, as the daemon last answered it: what "Restore defaults" shows.
    /// The daemon is the one that knows what the defaults are, and the dialog keeps no list of
    /// its own — a second copy is a copy that says the wrong thing the day the first one changes.
    property var defaultRules: []
    /// Set by "Restore defaults" and cleared by the first edit after it, because restoring is
    /// removing the file rather than writing those rules into one.
    property bool restoring: false
    /// What a refused save said, under the rows. Cleared by the next edit.
    property string error: ""
    /// Which rows Done will not take: a name with a `/` in it can never match, because a rule
    /// matches one name and not a path.
    property var bad: []
    /// The rules as they were when the dialog opened, so `saved` can say whether anything really
    /// changed: a plan on the Review screen is only stale if it is.
    property string asOpened: ""
    signal saved()

    readonly property var kinds: ["matches", "contains", "startsWith", "endsWith"]
    readonly property var kindLabels: ({ matches: "is", contains: "contains", startsWith: "starts with", endsWith: "ends with" })
    /// How tall the rules are, and how much of that the list shows: up to eight, then it scrolls.
    readonly property int rowsHeight: rows.length ? rows.length * 30 + (rows.length - 1) * 6 : 24
    readonly property int listHeight: Math.min(rowsHeight, 9 * 36 - 6)
    /// What a rule does, in one line, because "filter" says nothing about folders or deletes.
    readonly property string explanation:
        "A rule matches the NAME of a file or folder anywhere in the tree. A folder that matches is skipped with everything in it, and what is skipped is never deleted on the destination."

    function open() {
        error = ""; bad = []; restoring = false
        rows = []; asOpened = ""
        visible = true
        card.forceActiveFocus()
        Kiki.Daemon.request("MirrorFilters", {}, (ok, err) => {
            if (err) { error = err.message; return }
            dlg.take(ok)
            dlg.asOpened = dlg.stamp()
        })
    }
    /// The saved rules as one string, for telling a real change from a Done that changed nothing.
    /// The rules only, not where they came from: a scan does not care which file they are in.
    function stamp() { return rows.map(r => r.kind + " " + r.value).join("\n") }
    /// A reply from either request: the rules as they now stand, and the built-in set beside them.
    function take(ok) {
        const r = (ok && ok.rules) || []
        rows = r.map(x => ({ kind: x.kind, value: x.value }))
        defaults = ok && ok.defaults === true
        defaultRules = ((ok && ok.defaultRules) || []).map(x => ({ kind: x.kind, value: x.value }))
    }

    /// One edit, in one place, so every control clears the same three things.
    function edit(f) {
        const next = rows.map(r => ({ kind: r.kind, value: r.value }))
        f(next)
        rows = next; restoring = false; error = ""; bad = []
    }
    function cycleKind(i) { edit(n => { n[i].kind = kinds[(kinds.indexOf(n[i].kind) + 1) % kinds.length] }) }
    function setValue(i, v) { if (rows[i] && rows[i].value !== v) edit(n => { n[i].value = v }) }
    function removeRule(i) { edit(n => n.splice(i, 1)) }
    /// A new rule starts as `is` with nothing in it, and the cursor in it — and in view, or the
    /// ninth rule is typed into a box below the bottom of the list.
    function addRule() {
        edit(n => n.push({ kind: "matches", value: "" }))
        // Worked out from the rules rather than read off the Flickable: the row is in `rows`
        // already, but the Column it is drawn in has not grown yet.
        flick.contentY = Math.max(0, rowsHeight - listHeight)
        Qt.callLater(() => { const f = list.itemAt(dlg.rows.length - 1); if (f) f.focusValue() })
    }
    /// The built-in set, shown at once — the one the daemon sent, not a list kept here. Done takes
    /// the file away rather than writing those rules into one, and the answer to that is what
    /// fills the rows again; with no reply yet there is nothing to preview, and Done still restores.
    function restoreDefaults() {
        rows = defaultRules.map(r => ({ kind: r.kind, value: r.value }))
        restoring = true; error = ""; bad = []
    }

    /// Done. A row left empty is dropped rather than refused — an Add rule nobody filled in is
    /// not a mistake worth a message. A name with a `/` in it is, and it is marked where it is.
    function save() {
        if (restoring) {
            Kiki.Daemon.request("SetMirrorFilters", { defaults: true }, (ok, err) => dlg.wrote(ok, err))
            return
        }
        const keep = [], marks = []
        for (let i = 0; i < rows.length; i++) {
            const v = (rows[i].value || "").trim()
            if (!v) continue
            if (v.indexOf("/") >= 0) marks.push(i)
            else keep.push({ kind: rows[i].kind, value: v })
        }
        if (marks.length) { bad = marks; error = "A rule matches a name, not a path: take the / out."; return }
        Kiki.Daemon.request("SetMirrorFilters", { rules: keep }, (ok, err) => dlg.wrote(ok, err))
    }
    function wrote(ok, err) {
        if (err) { error = err.message; return }
        const was = asOpened
        take(ok)
        asOpened = stamp()
        visible = false
        // Only when the rules really changed: the rules are read at scan, so a plan on the Review
        // screen behind this dialog is stale — but not if Done saved what was already there.
        if (asOpened !== was) dlg.saved()
    }
    function cancel() { visible = false }

    /// Everything on screen, for scripts and tests.
    function info() {
        return { open: visible, rules: rows.map(r => ({ kind: r.kind, value: r.value })), defaults: defaults, error: error, bad: bad.slice() }
    }

    MouseArea { anchors.fill: parent; onClicked: dlg.cancel() }
    Rectangle {
        id: card
        objectName: "mirror-rules-card"
        anchors.centerIn: parent
        width: Math.min(560, parent.width - 32); height: Math.min(col.implicitHeight + 44, parent.height - 32)
        color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        focus: true
        Keys.onEscapePressed: dlg.cancel()
        MouseArea { anchors.fill: parent }
        Column {
            id: col
            x: 22; y: 22; width: parent.width - 44; spacing: 12
            Text { text: Kiki.T.tr("mirror.rulesTitle"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            Text {
                objectName: "mirror-rules-explanation"
                width: parent.width; wrapMode: Text.WordWrap; lineHeight: 1.3
                text: dlg.explanation; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
            // However many rules there are, the dialog stays the size it is and the list scrolls.
            Flickable {
                id: flick
                objectName: "mirror-rules-list"
                width: parent.width; height: dlg.listHeight
                clip: true; contentHeight: rowsCol.height
                NaturalScroll { }
                Column {
                    id: rowsCol; width: parent.width; spacing: 6
                    Repeater {
                        id: list
                        model: dlg.rows.length
                        delegate: Row {
                            id: ruleRow
                            required property int index
                            readonly property var rule: dlg.rows[index] || ({ kind: "matches", value: "" })
                            readonly property bool wrong: dlg.bad.indexOf(index) >= 0
                            function focusValue() { valueInput.forceActiveFocus() }
                            // Typing into a TextInput breaks the binding that filled it, so a row
                            // that takes the one below it — the x on the row above — would go on
                            // showing the name that was deleted. Put back by hand, and only when
                            // it really differs, or every keystroke would fight the cursor.
                            onRuleChanged: if (valueInput.text !== rule.value) valueInput.text = rule.value
                            width: rowsCol.width; height: 30; spacing: 8
                            // The kind, in the box the workspace's detector cycles in.
                            Rectangle {
                                objectName: "rule-kind-" + ruleRow.index
                                width: 130; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
                                Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: dlg.kindLabels[ruleRow.rule.kind]; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                                Icon { anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; name: "chev-d"; size: 12; color: Kiki.Theme.muted }
                                MouseArea { anchors.fill: parent; onClicked: dlg.cycleKind(ruleRow.index) }
                            }
                            // The name to match. A long one has to stay in its box, so it clips.
                            Rectangle {
                                width: ruleRow.width - 130 - 26 - 2 * ruleRow.spacing - (hint.visible ? hint.width + ruleRow.spacing : 0)
                                height: 30; radius: 2; clip: true
                                color: Kiki.Theme.bgDark; border.width: 1; border.color: ruleRow.wrong ? Kiki.Theme.danger : Kiki.Theme.gutter
                                TextInput {
                                    id: valueInput
                                    objectName: "rule-value-" + ruleRow.index
                                    anchors.fill: parent; anchors.margins: 8; clip: true
                                    verticalAlignment: TextInput.AlignVCenter
                                    activeFocusOnTab: true
                                    text: ruleRow.rule.value
                                    color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                                    selectionColor: Kiki.Theme.accent
                                    onTextChanged: dlg.setValue(ruleRow.index, text)
                                    // Enter moves on to the next field rather than closing the
                                    // dialog: a list is edited one row after another.
                                    onAccepted: nextItemInFocusChain().forceActiveFocus()
                                }
                            }
                            // `starts with .` is the hidden files in so many words.
                            Text {
                                id: hint
                                objectName: "rule-hint-" + ruleRow.index
                                visible: ruleRow.rule.kind === "startsWith" && ruleRow.rule.value === "."
                                anchors.verticalCenter: parent.verticalCenter
                                text: Kiki.T.tr("mirror.hiddenFiles"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
                            }
                            Rectangle {
                                objectName: "rule-remove-" + ruleRow.index
                                width: 26; height: 26; radius: 2; anchors.verticalCenter: parent.verticalCenter
                                color: removeHover.containsMouse ? Kiki.Theme.danger : "transparent"
                                border.width: 1; border.color: removeHover.containsMouse ? Kiki.Theme.danger : Kiki.Theme.gutter
                                activeFocusOnTab: true
                                Keys.onReturnPressed: dlg.removeRule(ruleRow.index)
                                Keys.onSpacePressed: dlg.removeRule(ruleRow.index)
                                Icon { anchors.centerIn: parent; name: "x"; size: 10; color: removeHover.containsMouse ? Kiki.Theme.bg : Kiki.Theme.fgDim }
                                MouseArea { id: removeHover; anchors.fill: parent; hoverEnabled: true; onClicked: dlg.removeRule(ruleRow.index) }
                            }
                        }
                    }
                    Text {
                        objectName: "mirror-rules-empty"
                        visible: dlg.rows.length === 0
                        text: Kiki.T.tr("mirror.noRules")
                        color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                    }
                }
            }
            Text {
                objectName: "mirror-rules-error"
                visible: dlg.error !== ""
                width: parent.width; wrapMode: Text.WordWrap
                text: dlg.error; color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: 12
            }
            Item {
                id: footer
                width: parent.width; height: 30
                Row {
                    spacing: 8; anchors.left: parent.left
                    Button { objectName: "mirror-rules-add"; text: Kiki.T.tr("mirror.addRule"); onClicked: dlg.addRule() }
                    Button { objectName: "mirror-rules-restore"; text: Kiki.T.tr("mirror.restoreDefaults"); onClicked: dlg.restoreDefaults() }
                }
                Row {
                    spacing: 8; anchors.right: parent.right
                    Button { objectName: "mirror-rules-cancel"; text: Kiki.T.tr("common.cancel"); onClicked: dlg.cancel() }
                    Button { objectName: "mirror-rules-done"; text: Kiki.T.tr("common.done"); primary: true; onClicked: dlg.save() }
                }
            }
        }
    }
}
