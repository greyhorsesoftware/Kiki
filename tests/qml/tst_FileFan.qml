import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

// What is carried, drawn: one card for one file, up to three fanned for several, a pill when
// three is not all of them, and a badge saying what will happen (owner, 2026-09-28).
TestCase {
    id: tc
    name: "FileFan"
    when: windowShown
    visible: true
    width: 300; height: 200

    Component { id: fanC; UI.FileFan {} }

    function cards(f) { const out = []; function walk(it) { for (const c of it.children) { if (c.objectName === "fan-card") out.push(c); walk(c) } } walk(f); return out }
    function part(f, name) { return findChild(f, name) }

    function test_one_file_is_one_card_with_no_pill_and_no_badge() {
        const f = fanC.createObject(tc, { rows: [{ kind: "image", thumb: "" }] })
        compare(cards(f).length, 1)
        verify(!part(f, "fan-count").visible, "no pill for one")
        verify(!part(f, "fan-badge").visible, "a move has no badge")
        f.destroy()
    }
    function test_three_files_are_three_cards_fanned() {
        const f = fanC.createObject(tc, { rows: [{ kind: "file" }, { kind: "image" }, { kind: "pdf" }] })
        const c = cards(f)
        compare(c.length, 3)
        verify(c[0].x < c[1].x && c[1].x < c[2].x, "fanned to the right")
        verify(c[0].rotation !== c[2].rotation, "and turned")
        verify(c[0].z > c[2].z, "the first on top")
        verify(!part(f, "fan-count").visible)
        f.destroy()
    }
    function test_more_than_three_shows_three_cards_and_the_count() {
        const f = fanC.createObject(tc, { rows: [{ kind: "file" }, { kind: "file" }, { kind: "file" }], count: 12 })
        compare(cards(f).length, 3, "never more than three cards")
        verify(part(f, "fan-count").visible, "the pill says how many")
        compare(part(f, "fan-count").children[0].text, "12")
        f.destroy()
    }
    function test_the_badge_says_what_will_happen() {
        const f = fanC.createObject(tc, { rows: [{ kind: "file" }], badge: "+" })
        verify(part(f, "fan-badge").visible)
        compare(part(f, "fan-badge").children[0].text, "+")
        f.badge = ""
        verify(!part(f, "fan-badge").visible)
        f.destroy()
    }
    function test_a_count_with_no_rows_still_draws_cards() {
        // A paste knows how many it carries and nothing of what they are.
        const f = fanC.createObject(tc, { rows: [], count: 2 })
        compare(cards(f).length, 2)
        f.destroy()
    }

    // The count sits bottom right (owner, 2026-09-28), the badge in the other corner.
    function test_the_count_pill_sits_bottom_right() {
        const fan = fanC.createObject(tc)
        fan.rows = [{ kind: "file", thumb: "" }, { kind: "file", thumb: "" }, { kind: "file", thumb: "" }]; fan.count = 12; fan.badge = "+"
        const pill = findChild(fan, "fan-count"), badge = findChild(fan, "fan-badge")
        verify(pill !== null && badge !== null)
        verify(pill.y + pill.height / 2 > fan.height / 2, "the pill is in the lower half")
        verify(pill.x + pill.width / 2 > fan.width / 2, "and the right half")
        verify(badge.x + badge.width / 2 < fan.width / 2, "the badge keeps the left")
        fan.destroy()
    }

}
