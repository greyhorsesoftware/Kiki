import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI

// The video face of Quick Look (docs/0.2.0/05-quicklook.md, decision 3), as far as it goes
// without a decoder: the clock's words, the keys whose effect shows without a frame (`m`), the
// bar's labels through the catalog, and the player's message when the file will not play.
TestCase {
    id: tc
    name: "QuickLookVideo"
    when: windowShown
    visible: true
    width: 480; height: 320

    property var video: null
    Component { id: videoC; UI.QuickLookVideo { anchors.fill: parent } }

    function init() { Kiki.T.language = "en" }
    function cleanup() {
        if (video) { video.destroy(); video = null }
        Kiki.T.language = "en"
    }
    function open(uri) {
        video = videoC.createObject(tc, { uri: uri || "" })
        video.forceActiveFocus()
        return video
    }

    function test_formatTime() {
        open()
        compare(video.formatTime(0), "0:00")
        compare(video.formatTime(999), "0:00")
        compare(video.formatTime(7000), "0:07")
        compare(video.formatTime(65000), "1:05")
        compare(video.formatTime(220000), "3:40")
        compare(video.formatTime(3725000), "1:02:05")
        compare(video.formatTime(-5), "0:00")
        compare(video.formatTime(undefined), "0:00")
    }

    function test_the_clock_reads_position_over_duration() {
        open()
        compare(findChild(video, "quicklook-video-clock").text, "0:00 / 0:00")
    }

    function test_m_mutes_and_unmutes() {
        open()
        verify(!video.muted)
        keyClick(Qt.Key_M)
        verify(video.muted)
        keyClick(Qt.Key_M)
        verify(!video.muted)
    }

    function test_the_mute_button_toggles_too() {
        open()
        const mute = findChild(video, "quicklook-video-mute")
        mouseClick(mute)
        verify(video.muted)
        mouseClick(mute)
        verify(!video.muted)
    }

    function test_the_windows_keys_are_left_alone() {
        open()
        // Space and Esc are the window's; a key this face does not take must not be eaten.
        const catcher = Qt.createQmlObject("import QtQuick; Item { Keys.onPressed: event => { if (event.key === Qt.Key_Escape) parent.escaped = true } }", tc)
        tc.escaped = false
        video.parent = catcher
        video.forceActiveFocus()
        keyClick(Qt.Key_Escape)
        verify(tc.escaped, "Esc did not reach the parent")
        keyClick(Qt.Key_M)
        verify(video.muted)
        video.parent = tc
        catcher.destroy()
    }
    property bool escaped: false

    function test_the_bar_speaks_the_language() {
        open()
        const playTip = findChild(video, "quicklook-video-play-tip"), muteTip = findChild(video, "quicklook-video-mute-tip")
        verify(playTip && muteTip, "no tips on the buttons")
        compare(playTip.text, "Play")              // nothing is playing without a file
        compare(muteTip.text, "Mute")
        video.muted = true
        compare(muteTip.text, "Unmute")
        Kiki.T.language = "es"
        compare(playTip.text, "Reproducir")
        compare(muteTip.text, "Activar el sonido")
        video.muted = false
        compare(muteTip.text, "Silenciar")
        Kiki.T.language = "ja"
        compare(playTip.text, "再生")
        compare(muteTip.text, "ミュート")
    }

    // A file that is not there is refused by the backend before any decoding: the one failure
    // that needs no decoder to reach the screen.
    function test_a_file_that_will_not_play_shows_the_players_message() {
        open("file:///nowhere/at/all.mp4")
        const said = findChild(video, "quicklook-video-error")
        tryVerify(() => said.visible, 4000, "the player said nothing about a file it cannot play")
        verify(said.text.length > 0)
    }
}
