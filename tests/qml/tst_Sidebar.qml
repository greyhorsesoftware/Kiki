import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

// The rail's first entry is Search everywhere (0.1.1): above Home, not a favorite, lit while the
// overlay is up, and where Ctrl+B lands first.
TestCase {
    name: "Sidebar"
    when: windowShown
    visible: true
    width: 224; height: 400
    UI.Sidebar {
        id: sidebar
        width: 224; height: 400
        favorites: [{ name: "Home", uri: "file:///home/t" }, { name: "Documents", uri: "file:///home/t/Documents" }]
        locations: [{ name: "lab", plugin: "sftp", remoteUri: "sftp://lab/", connected: false }]
    }
    SignalSpy { id: searches; target: sidebar; signalName: "searchRequested" }
    SignalSpy { id: opens; target: sidebar; signalName: "open" }
    readonly property var twoFavorites: [{ name: "Home", uri: "file:///home/t" }, { name: "Documents", uri: "file:///home/t/Documents" }]
    function init() { searches.clear(); opens.clear(); sidebar.keyIndex = -1; sidebar.searchOpen = false }
    function cleanup() { sidebar.height = 400; sidebar.favorites = twoFavorites }

    // Too short a window for the rail (owner, 2026-09-24): what does not fit is under the
    // bottom edge, and the wheel brings it up.
    function test_a_short_rail_scrolls_to_what_is_below_the_edge() {
        sidebar.favorites = Array.from({ length: 12 }, (_, i) => ({ name: "f" + i, uri: "file:///home/t/f" + i }))
        sidebar.height = 160; wait(30)
        const scroll = findChild(sidebar, "sidebar-scroll")
        verify(scroll.contentHeight > sidebar.height, "more than fits: " + scroll.contentHeight)
        const last = findChild(sidebar, "sidebar-f11")
        verify(last, "the last favorite is drawn")
        verify(last.mapToItem(sidebar, 0, 0).y > sidebar.height, "the last favorite is below the edge")
        mouseWheel(scroll, 20, 80, 0, -120)
        tryVerify(() => scroll.contentY > 0, 1000, "the wheel scrolls it")
        // (A synthetic wheel is one small flick; the strip's end is reached by scrolling there.)
        scroll.contentY = scroll.contentHeight - scroll.height
        tryVerify(() => findChild(sidebar, "sidebar-f11").mapToItem(sidebar, 0, 0).y + 20 <= sidebar.height, 1000, "and the last favorite is within the strip at the end")
    }
    function test_search_is_the_first_entry_above_home() {
        compare(sidebar.entries[0].kind, "search")
        compare(sidebar.entries[1].kind, "favorite")
        compare(sidebar.entries[1].item.name, "Home")
        const search = findChild(sidebar, "sidebar-search")
        verify(search !== null, "the rail draws it")
        const home = findChild(sidebar, "sidebar-favorite-0")
        verify(home === null || search.mapToItem(sidebar, 0, 0).y < home.mapToItem(sidebar, 0, 0).y, "above Home")
    }
    function test_a_click_asks_for_the_search_and_opens_nothing() {
        const search = findChild(sidebar, "sidebar-search")
        mouseClick(search, search.width / 2, search.height / 2)
        compare(searches.count, 1)
        compare(opens.count, 0)
    }
    function test_ctrl_b_lands_on_it_first_and_enter_opens_the_search() {
        sidebar.moveKey(1)
        compare(sidebar.keyIndex, 0)
        verify(findChild(sidebar, "sidebar-search").keyed)
        sidebar.activateKey()
        compare(searches.count, 1)
        sidebar.moveKey(1)
        sidebar.activateKey()
        compare(opens.count, 1, "the next entry is Home")
        compare(opens.signalArguments[0][0], "file:///home/t")
    }
    function test_the_favorites_key_indexes_moved_along_by_one() {
        compare(sidebar.keyOffset("favorite", 0), 1)
        compare(sidebar.keyOffset("trash", 0), 3)
        compare(sidebar.keyOffset("location", 0), 4)
    }
    function test_it_is_lit_while_the_overlay_is_up() {
        const search = findChild(sidebar, "sidebar-search")
        verify(!search.active)
        sidebar.searchOpen = true
        verify(search.active)
    }
}
