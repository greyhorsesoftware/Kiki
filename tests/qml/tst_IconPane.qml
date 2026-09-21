import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// Icon view under the mouse (plan 28): the same selection and activation contract as the list,
// plus the zoom that plan 23 put on pinch and Ctrl+wheel.
TestCase {
    id: tc
    name: "IconPane"
    when: windowShown
    visible: true
    width: 700; height: 400

    property var fake: null
    property var pane: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        anchors.fill: parent
        Views.IconPane { id: icons; anchors.fill: parent; pane: tc.pane }
    }
    SignalSpy { id: activated; target: icons; signalName: "activate" }
    SignalSpy { id: menued; target: icons; signalName: "contextMenu" }

    function init() {
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.dir("Projects"), fake.file("a.txt"), fake.file("b.txt")] }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        icons.pane = pane
        pane.open("file:///home/t")
        wait(50)
        activated.clear(); menued.clear()
    }
    function cleanup() { icons.pane = null; pane.destroy(); fake.destroy() }

    function test_a_tile_per_row_in_the_listing() {
        compare(pane.listing.count, 3)
        verify(findChild(icons, "tile-0") !== null)
        verify(findChild(icons, "tile-2") !== null)
    }

    // Between the icons there is no tile to click, and that has to give the folder's menu rather
    // than nothing at all.
    function test_right_click_between_the_icons_asks_for_the_folder_menu() {
        pane.selection.set(2)
        mouseClick(icons, tc.width / 2, tc.height - 20, Qt.RightButton)
        compare(menued.count, 1)
        compare(menued.signalArguments[0][0], -1)
        compare(pane.selection.count(), 0)
    }

    function test_click_selects_that_tile() {
        const tile = findChild(icons, "tile-1")
        mouseClick(tile, tile.width / 2, tile.height / 2)
        compare(pane.selection.current, 1)
    }

    function test_ctrl_click_adds_to_the_selection() {
        const a = findChild(icons, "tile-0"), b = findChild(icons, "tile-2")
        mouseClick(a, a.width / 2, a.height / 2)
        mouseClick(b, b.width / 2, b.height / 2, Qt.LeftButton, Qt.ControlModifier)
        compare(pane.selection.count(), 2)
        verify(pane.selection.has(0))
        verify(pane.selection.has(2))
    }

    function test_double_click_activates_that_tile() {
        const tile = findChild(icons, "tile-2")
        mouseDoubleClickSequence(tile, tile.width / 2, tile.height / 2)
        compare(activated.count, 1)
        compare(activated.signalArguments[0][0], 2)
    }

    function test_right_click_asks_for_the_context_menu() {
        const tile = findChild(icons, "tile-1")
        mouseClick(tile, tile.width / 2, tile.height / 2, Qt.RightButton)
        compare(menued.count, 1)
        compare(menued.signalArguments[0][0], 1)
    }

    // Zooming grows the cells; the padding around the glyph stays put, so tiles do not drift
    // apart as they grow (plan 23).
    function test_zoom_grows_the_cell_by_the_glyph_alone() {
        pane.iconZoom = 1
        const base = icons.iconSize, w1 = icons.cellW, h1 = icons.cellH
        pane.iconZoom = 2
        compare(icons.iconSize, base * 2)
        compare(icons.cellH - h1, base)                    // the cell grows by the icon alone
        verify(icons.cellW >= w1)
        compare(icons.cellH - icons.iconSize, 72)          // padding is constant
    }

    // Nautilus opens a folder at 96px, and so does this.
    function test_the_default_icon_is_96px() {
        pane.iconZoom = 1
        compare(icons.iconSize, 96)
    }

    // ---------------------------------------------------------------- the lasso
    // Coordinates are the pane's; the grid sits 18 px in. Three tiles in one row, so everything
    // under that row is empty space.
    function at(col, row, dx, dy) { return Qt.point(18 + col * icons.cellW + (dx || 0), 18 + row * icons.cellH + (dy || 0)) }
    function drag(from, to, modifiers) {
        mousePress(icons, from.x, from.y, Qt.LeftButton, modifiers || Qt.NoModifier)
        const steps = 6
        for (let k = 1; k <= steps; k++) mouseMove(icons, from.x + (to.x - from.x) * k / steps, from.y + (to.y - from.y) * k / steps, -1, Qt.LeftButton)
    }
    function drop(to) { mouseRelease(icons, to.x, to.y, Qt.LeftButton) }

    function test_a_band_from_empty_space_selects_what_it_touches() {
        const from = at(0, 1, 6, 40), to = at(1, 0, icons.cellW / 2, icons.cellH / 2)
        drag(from, to)
        verify(icons.lassoing)
        verify(findChild(icons, "icon-lasso-band").visible)
        compare(pane.selection.positions(), [0, 1])
        // Wider, and the third comes in; back again, and it goes out: the band is live.
        const wide = at(2, 0, icons.cellW / 2, icons.cellH / 2)
        mouseMove(icons, wide.x, wide.y, -1, Qt.LeftButton)
        compare(pane.selection.positions(), [0, 1, 2])
        mouseMove(icons, to.x, to.y, -1, Qt.LeftButton)
        compare(pane.selection.positions(), [0, 1])
        drop(to)
        verify(!icons.lassoing)
        verify(!findChild(icons, "icon-lasso-band").visible)
        compare(pane.selection.positions(), [0, 1], "letting go keeps what the band held")
    }
    function test_ctrl_adds_to_the_selection_and_toggles_what_was_in_it() {
        pane.selection.set(0)
        const from = at(0, 1, 6, 40), to = at(1, 0, icons.cellW / 2, icons.cellH / 2)
        drag(from, to, Qt.ControlModifier)
        compare(pane.selection.positions(), [1], "0 was selected and is touched again: out; 1 comes in")
        drop(to)
        pane.selection.set(2)
        drag(from, at(0, 0, icons.cellW / 2, icons.cellH / 2), Qt.ControlModifier)
        compare(pane.selection.positions(), [0, 2])
        drop(to)
    }
    function test_a_click_on_empty_space_lets_go_of_the_selection() {
        pane.selection.set(1)
        const p = at(0, 1, 30, 60)
        mouseClick(icons, p.x, p.y)
        compare(pane.selection.count(), 0)
    }
    function test_a_press_on_a_picture_is_the_files_not_the_bands() {
        const p = at(1, 0, icons.cellW / 2, 14 + 4 + icons.iconSize / 2)
        mousePress(icons, p.x, p.y)
        mouseMove(icons, p.x + 3, p.y + 3, -1, Qt.LeftButton)
        verify(!icons.lassoing)
        mouseRelease(icons, p.x + 3, p.y + 3)
        compare(pane.selection.positions(), [1])
    }
    function test_the_gutter_between_two_tiles_selects_neither() {
        const x = icons.cellW                                   // the line between columns 0 and 1
        compare(icons.tilesIn(Qt.rect(x - 4, 0, 8, icons.cellH)), [])
        compare(icons.tilesIn(Qt.rect(x - 4, 0, 40, icons.cellH)), [1])
    }
    function test_rows_far_off_screen_are_in_the_band_too() {
        const rows = []
        for (let i = 0; i < 500; i++) rows.push(fake.file("f" + i + ".txt"))
        fake.tree = { "file:///home/many": rows }
        pane.open("file:///home/many")
        wait(80)
        const per = icons.perRow
        const hit = icons.tilesIn(Qt.rect(0, 0, icons.cellW * per, icons.cellH * 40))
        compare(hit.length, per * 40)
        compare(hit[hit.length - 1], per * 40 - 1)
        // …and never past the end of the folder.
        compare(icons.tilesIn(Qt.rect(0, 0, icons.cellW * per, icons.cellH * 4000)).length, 500)
    }
}
