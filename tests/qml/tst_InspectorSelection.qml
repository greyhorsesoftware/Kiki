import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI
import KikiTest

// The info panel over a selection of many (0.1.1): a count and a fan where the icon and the
// preview were, fields summed and merged, a permissions grid that says where the items differ,
// and an Apply that sends only the bits touched, for every item, as one job.
TestCase {
    id: tc
    name: "InspectorSelection"
    when: windowShown
    visible: true
    width: 420; height: 900

    readonly property var three: [
        { name: "a.txt", kind: "text", isDir: false, meta: { size: 10, mtime: 1700000000000, mode: 0o644, owner: "gideon", group: "users" } },
        { name: "b.sh", kind: "code", isDir: false, meta: { size: 20, mtime: 1700000500000, mode: 0o755, owner: "gideon", group: "users" }, git: { state: "modified" } },
        { name: "c.key", kind: "file", isDir: false, meta: { size: 30, mtime: 1700000200000, mode: 0o600, owner: "gideon", group: "wheel" } },
    ]
    UI.Inspector {
        id: insp
        anchors.fill: parent
        standalone: true
        uri: "file:///home/t/a.txt"
        row: tc.three[0]
        rows: tc.three
    }
    SignalSpy { id: manys; target: insp; signalName: "chmodMany" }
    SignalSpy { id: ones; target: insp; signalName: "chmod" }
    function init() { Wire.reset(); insp.rows = tc.three; insp.tab = "general"; insp.touchedMask = 0; insp.touchedBits = 0; manys.clear(); ones.clear(); wait(20) }
    function field(name) { return findChild(insp, name) }

    function test_the_header_counts_and_the_preview_is_a_fan_of_the_kinds() {
        verify(insp.many)
        compare(findChild(insp, "inspector-title").children[0].text, "3 items")
        verify(findChild(insp, "inspector-count").visible, "the count on its card")
        verify(!findChild(insp, "inspector-title-icon").visible, "no one icon")
        const fan = findChild(insp, "inspector-fan")
        verify(fan.visible)
        compare(fan.kinds, ["text", "code", "file"])
        compare(fan.shown, 3)
        const box = findChild(insp, "inspector-preview")
        compare(box.border.width, 0, "no box around the fan")
        compare(Wire.count("Preview"), 0, "nothing is asked of the daemon for a selection")
    }
    // Three of one kind are three cards: the fan is of the items (owner, 2026-09-24: a
    // selection of folders on the local side showed no fan at all).
    function test_one_kind_many_times_is_still_a_fan() {
        insp.rows = ["a", "b", "c"].map(n => ({ name: n, kind: "folder", isDir: true, meta: { size: 0, mtime: 1, mode: 0o755 } })); wait(20)
        const fan = findChild(insp, "inspector-fan")
        compare(fan.kinds, ["folder"])
        compare(fan.shown, 3)
        compare(fan.cards, ["folder", "folder", "folder"])
        verify(!findChild(fan, "fan-more").visible)
    }
    function test_more_kinds_than_cards_is_a_plus_n_at_the_fans_foot() {
        const kinds = ["text", "code", "image", "archive", "music", "video", "doc"]
        insp.rows = kinds.map(k => ({ name: k, kind: k, isDir: false, meta: { size: 1, mtime: 1, mode: 0o644 } })); wait(20)
        const fan = findChild(insp, "inspector-fan"), more = findChild(fan, "fan-more")
        compare(fan.shown, 5)
        verify(more.visible)
        compare(more.text, "+2")
        verify(Math.abs((more.x + more.width / 2) - fan.width / 2) < 1, "centred: " + more.x + "+" + more.width + " in " + fan.width)
        compare(more.y + more.height, fan.height, "at the foot")
    }
    function test_the_general_fields_are_summed_and_merged() {
        compare(field("insp-type").label, "Kinds")
        compare(field("insp-type").value, "1 Text, 1 Code, and 1 more")
        compare(field("insp-size").value, "60 B (60 bytes)")
        verify(field("insp-modified").value.endsWith("(newest)"))
        compare(field("insp-owner").value, "gideon")
        compare(field("insp-group").value, "mixed")
        verify(field("insp-git").visible)
        compare(field("insp-git").value, "2 clean, 1 modified")
    }
    function test_the_grid_shows_where_the_items_differ() {
        insp.tab = "permissions"; wait(20)
        compare(findChild(insp, "perm-owner-4").state, "on", "everyone can read as owner")
        compare(findChild(insp, "perm-owner-1").state, "mixed", "only b.sh is executable")
        compare(findChild(insp, "perm-world-4").state, "mixed", "c.key is 600")
        compare(findChild(insp, "perm-world-2").state, "off")
        compare(field("perm-octal").value, "mixed (644, 755, 600)")
        verify(!findChild(insp, "perm-apply").enabled, "nothing touched yet")
    }
    function test_a_click_sets_a_mixed_bit_for_all_and_apply_sends_only_what_was_touched() {
        insp.tab = "permissions"; wait(20)
        mouseClick(findChild(insp, "perm-owner-1"))
        compare(findChild(insp, "perm-owner-1").state, "on")
        mouseClick(findChild(insp, "perm-owner-4"))
        compare(findChild(insp, "perm-owner-4").state, "off", "an on bit clicked is cleared for all")
        compare(findChild(insp, "perm-apply").text, "Apply", "just Apply (owner, 2026-09-24): the header already says how many")
        verify(findChild(insp, "perm-apply").enabled)
        mouseClick(findChild(insp, "perm-apply"))
        compare(manys.count, 1)
        compare(manys.signalArguments[0][0], 0o500, "the two bits touched")
        compare(manys.signalArguments[0][1], 0o100, "exec on, read off")
        compare(manys.signalArguments[0][2], false)
        compare(ones.count, 0, "not the single-file path")
    }
    function test_revert_forgets_what_was_touched() {
        insp.tab = "permissions"; wait(20)
        verify(!findChild(insp, "perm-revert").lit, "nothing touched: the mark is dim")
        mouseClick(findChild(insp, "perm-group-2"))
        verify(findChild(insp, "perm-revert").lit, "touched: lit")
        mouseClick(findChild(insp, "perm-revert"))
        compare(insp.touchedMask, 0)
        verify(!findChild(insp, "perm-revert").lit)
        compare(findChild(insp, "perm-group-2").state, "off")
    }
    function test_one_row_again_is_the_panel_as_it_was() {
        insp.rows = []
        verify(!insp.many)
        compare(findChild(insp, "inspector-title").children[0].text, "a.txt")
        verify(findChild(insp, "inspector-title-icon").visible)
        verify(!findChild(insp, "inspector-fan").visible)
        compare(field("insp-type").label, "Type")
        compare(field("insp-size").value, "10 B (10 bytes)")
    }
    // The card opened too tall over a selection and shrank on the switch to Permissions: the
    // unseen copy of the other tab, made while one file was shown, kept that file's layout
    // (2026-09-24). Now the two tabs measure the same whichever is up, after the rows change.
    function test_the_compact_card_keeps_one_height_across_tabs_after_the_rows_change() {
        insp.rows = []; insp.compact = true; wait(20)
        const one = insp.naturalHeight
        insp.rows = tc.three; wait(20)
        const onGeneral = insp.naturalHeight
        insp.tab = "permissions"; wait(20)
        compare(insp.naturalHeight, onGeneral, "the same height on Permissions as on General")
        insp.tab = "general"; wait(20)
        compare(insp.naturalHeight, onGeneral)
        verify(onGeneral < one, "a selection has no Symbolic row: shorter than one file (" + onGeneral + " vs " + one + ")")
        insp.compact = false
    }
    function test_the_compact_shape_has_no_preview_and_no_grip() {
        insp.compact = true; wait(20)
        compare(findChild(insp, "inspector-preview").height, 0)
        verify(!findChild(insp, "inspector-grip").visible)
        verify(insp.naturalHeight > 200 && insp.naturalHeight < 600, "as tall as its content: " + insp.naturalHeight)
        // A smaller header and Apply in the card (owner, 2026-09-24).
        compare(findChild(insp, "inspector-mini-fan").height, 36, "the fan keeps its size: smaller it is a smudge (owner, 2026-09-24)")
        insp.rows = []; wait(20)
        compare(findChild(insp, "inspector-title-icon").size, 24)
        compare(findChild(insp, "inspector-title").children[0].font.pixelSize, 13)
        insp.rows = tc.three; wait(20)
        // Owner and group share a line in the card, on both tabs (owner, 2026-09-24).
        compare(field("insp-owner").label, "Owner/Group")
        compare(field("insp-owner").value, "gideon · mixed")
        verify(!field("insp-group").visible)
        insp.tab = "permissions"; wait(20)
        compare(field("perm-owner").value, "gideon · mixed")
        verify(!field("perm-group").visible)
        compare(findChild(insp, "perm-apply").height, 24, "a small Apply in the card")
        insp.compact = false; insp.tab = "general"; wait(20)
        compare(field("insp-owner").label, "Owner")
        compare(field("insp-owner").value, "gideon", "the panel: its own rows again")
        verify(field("insp-group").visible)
    }
}
