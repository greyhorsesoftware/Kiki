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
        compare(icons.cellH - icons.iconSize, 66)          // padding is constant
    }

    // Nautilus opens a folder at 96px, and so does this.
    function test_the_default_icon_is_96px() {
        pane.iconZoom = 1
        compare(icons.iconSize, 96)
    }
}
