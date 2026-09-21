import QtQuick
import QtTest

// The info panel's player, with a real three-second clip (`fixtures/clip.mp4`, made with ffmpeg's
// `testsrc`). The panel builds it on the first click and hands it the file afterwards, as here.
// It used to ask for `play()` the moment it existed — before it had a file — and a player with
// nothing in it forgets the request: the first click on the play button did nothing at all.
TestCase {
    name: "VideoPreview"
    when: windowShown
    visible: true
    width: 200; height: 200

    Loader {
        id: video
        active: false
        source: "../../qml/kiki/ui/VideoPreview.qml"
        onLoaded: item.source = Qt.resolvedUrl("fixtures/clip.mp4")
    }
    function cleanup() { video.active = false }

    function test_it_plays_from_the_click_that_built_it() {
        video.active = true
        tryVerify(() => video.item && video.item.playing, 4000, "built, given its file, and never started")
    }
    function test_toggle_pauses_and_plays_again() {
        video.active = true
        tryVerify(() => video.item && video.item.playing, 4000)
        video.item.toggle()
        tryVerify(() => !video.item.playing)
        video.item.toggle()
        tryVerify(() => video.item.playing)
    }
}
