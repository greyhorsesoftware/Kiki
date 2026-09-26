import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/viewmenu.js" as ViewMenu

// The view menu ticks the view of the pane it is about. Side by side it ticked nothing at all.
TestCase {
    name: "ViewMenu"

    function ticked(view, hidden) { return ViewMenu.items(view, hidden, Kiki.T.tr).filter(i => i.checked).map(i => i.id) }

    function test_exactly_the_current_view_is_ticked_data() {
        return [{ tag: "icon", view: "icon" }, { tag: "list", view: "list" }, { tag: "columns", view: "columns" }, { tag: "gallery", view: "gallery" }]
    }
    function test_exactly_the_current_view_is_ticked(d) { compare(ticked(d.view, false), [d.view]) }

    function test_hidden_files_is_a_tick_of_its_own() {
        compare(ticked("list", true), ["list", "hidden"])
        compare(ticked("list", undefined), ["list"], "unset is off, not on")
    }
    function test_a_view_it_does_not_know_ticks_nothing() {
        compare(ticked("mirror", false), [], "an old preference naming a view that is gone")
    }
    function test_the_rows_and_their_keys() {
        const it = ViewMenu.items("list", false, Kiki.T.tr)
        compare(it.map(i => i.label), ["Icon", "List", "Columns", "Gallery", "Show hidden files"])
        compare(it.map(i => i.key), ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+5", "Ctrl+H"])
        verify(it[4].sep === true && it[0].sep === undefined, "the divider is above hidden files only")
    }
}
