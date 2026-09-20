import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI

// "Send via Tailscale ▸" — a submenu whose list is asked for when the row is opened and arrives
// later. The targets used to come up as a second menu at the top of the window.
TestCase {
    id: tc
    name: "MenuLazySub"
    when: windowShown
    visible: true
    width: 700; height: 500

    UI.ContextMenu { id: menu }
    property int asked: 0
    property var fill: null
    property string sent: ""

    function items() {
        return [{ label: "Open", action: () => {} },
                { label: "Send via Tailscale", sep: true, icon: "cloud", items: [{ label: "Looking…", enabled: false, action: () => {} }], load: f => { tc.asked++; tc.fill = f } },
                { label: "Send via Mail", icon: "mail", action: () => tc.sent = "mail" },
                { label: "Send via Gone", enabled: false, key: "not installed" }]
    }
    function init() {
        asked = 0; fill = null; sent = ""
        mouseMove(tc, 650, 450)          // off the menu: a pointer already over a row never "enters" it
        menu.open(items(), Qt.point(40, 40))
        // Until the Column has laid its rows out they lie on top of each other, and a click meant
        // for one lands on another.
        tryVerify(() => menu.box.height > 100)
    }
    function cleanup() { menu.close(); menu.items = []; wait(30) }   // and the old rows are really gone
    function row(label) { return findChild(menu, "menu-" + label) }

    function test_opening_the_row_asks_and_shows_something_meanwhile() {
        compare(asked, 0, "not before it is wanted")
        mouseMove(row("Send via Tailscale"), 20, 12)
        tryVerify(() => tc.asked === 1)
        tryVerify(() => row("Looking…") !== null)
        const sub = row("Looking…").parent.parent
        verify(sub.x >= menu.box.x + menu.box.width - 4, "beside the menu, not somewhere else in the window")
    }
    function test_the_list_replaces_it_and_is_asked_for_once() {
        mouseMove(row("Send via Tailscale"), 20, 12)
        tryVerify(() => tc.fill !== null)
        tc.fill([{ label: "phone", action: () => tc.sent = "phone" }, { label: "laptop", enabled: false, key: "offline" }])
        tryVerify(() => row("phone") !== null)
        mouseMove(row("Send via Mail"), 20, 12)
        tryVerify(() => menu.subItems.length === 0)
        mouseMove(row("Send via Tailscale"), 20, 12)
        tryVerify(() => row("phone") !== null)
        compare(asked, 1)
        tryVerify(() => row("laptop") && row("laptop").y > row("phone").y)     // laid out, not stacked
        mouseClick(row("laptop"))
        verify(menu.visible, "an offline target does nothing")
        mouseClick(row("phone"))
        compare(sent, "phone")
        verify(!menu.visible)
    }
    function test_an_answer_for_a_menu_that_has_closed_is_dropped() {
        mouseMove(row("Send via Tailscale"), 20, 12)
        tryVerify(() => tc.fill !== null)
        menu.close()
        tc.fill([{ label: "phone", action: () => {} }])
        compare(menu.subItems.length, 0)
    }
    function test_a_way_that_is_not_installed_is_there_but_does_nothing() {
        mouseClick(row("Send via Gone"))
        verify(menu.visible)
        mouseClick(row("Send via Mail"))
        compare(sent, "mail")
    }
}
