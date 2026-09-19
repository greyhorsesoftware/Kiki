import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/views" as Views
import KikiTest

// The gallery's slideshow controls float over the picture while the pointer is on it, like the
// info panel's video buttons, instead of holding a bar of their own under it.
TestCase {
    id: tc
    name: "GalleryPane"
    when: windowShown
    visible: true
    width: 800; height: 600

    property var fake: null
    property var pane: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: paneC; Kiki.Pane {} }

    Item {
        anchors.fill: parent
        Views.GalleryPane { id: gallery; anchors.fill: parent; pane: tc.pane }
    }

    function init() {
        Kiki.Settings.viewPrefs = ({})
        Wire.reset()
        fake = fakeC.createObject(tc)
        fake.tree = { "file:///home/t": [fake.file("a.jpg"), fake.file("b.jpg")] }
        pane = paneC.createObject(tc)
        pane.listing.daemon = fake
        gallery.pane = pane
        pane.open("file:///home/t")
        wait(50)
        mouseMove(gallery, gallery.width / 2, gallery.height - 4)     // on the filmstrip
    }
    function cleanup() { gallery.pane = null; pane.destroy(); fake.destroy() }

    function test_the_pill_shows_only_with_the_pointer_on_the_picture() {
        const pill = findChild(gallery, "gallery-pill")
        const play = findChild(gallery, "gallery-play")
        tryVerify(() => !pill.visible && !play.visible)
        mouseMove(gallery, gallery.width / 2, 40)                      // on the picture
        tryVerify(() => pill.visible && play.visible)
        // …and stays while the pointer is on the buttons themselves.
        mouseMove(play, play.width / 2, play.height / 2)
        verify(pill.visible && play.visible)
        mouseMove(gallery, gallery.width / 2, gallery.height - 4)
        tryVerify(() => !pill.visible)
    }

    function test_the_pill_sits_in_the_middle_of_the_picture() {
        mouseMove(gallery, gallery.width / 2, 40)
        const pill = findChild(gallery, "gallery-pill")
        tryVerify(() => pill.visible)
        const mid = pill.mapToItem(gallery, pill.width / 2, pill.height / 2)
        verify(Math.abs(mid.x - gallery.width / 2) <= 1, "x " + mid.x)
        verify(Math.abs(mid.y - gallery.stageHeight / 2) <= 1, "y " + mid.y)
    }

    // Over a picture a grey box behind the icon is noise: the icon lights instead.
    function test_a_button_lights_its_icon_not_a_box_behind_it() {
        mouseMove(gallery, gallery.width / 2, 40)
        const play = findChild(gallery, "gallery-play")
        const icon = findChild(play, "icon")
        tryVerify(() => play.visible)
        verify(icon.size >= 24)
        verify(Qt.colorEqual(icon.color, "white"))
        mouseMove(play, play.width / 2, play.height / 2)
        tryVerify(() => play.hovered)
        verify(Qt.colorEqual(icon.color, Kiki.Theme.accent))
        verify(Qt.colorEqual(play.color, "transparent"))
    }

    function test_play_still_toggles_the_slideshow() {
        mouseMove(gallery, gallery.width / 2, 40)
        const play = findChild(gallery, "gallery-play")
        tryVerify(() => play.visible)
        compare(gallery.playing, false)
        mouseClick(play, play.width / 2, play.height / 2)
        compare(gallery.playing, true)
        gallery.playing = false
    }

    // The options hang off the pill: with them open it must not vanish from under them.
    function test_open_options_keep_the_pill_up() {
        mouseMove(gallery, gallery.width / 2, 40)
        const gear = findChild(gallery, "gallery-settings")
        tryVerify(() => gear.visible)
        mouseClick(gear, gear.width / 2, gear.height / 2)
        verify(findChild(gallery, "gallery-options").visible)
        mouseMove(gallery, gallery.width / 2, gallery.height - 4)
        wait(50)
        verify(findChild(gallery, "gallery-pill").visible)
        findChild(gallery, "gallery-options").visible = false
    }

    // No bar under the picture any more: it has the whole height above the filmstrip.
    function test_the_picture_gets_the_room_the_bar_used_to_take() {
        compare(gallery.stageHeight, gallery.height - gallery.stripHeight)
    }
}
