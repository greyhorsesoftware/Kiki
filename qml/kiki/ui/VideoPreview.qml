import QtQuick
import QtMultimedia

// A video playing inside the info panel's preview box. Built only once somebody clicks the still
// — until then the panel costs a video nothing, and QtMultimedia is never loaded. It starts
// playing as soon as it exists; `playing` and `toggle()` are what the play/pause button drives.
Item {
    id: vp
    property url source
    readonly property bool playing: player.playbackState === MediaPlayer.PlayingState
    /// True once a frame is on screen, so the still underneath can stay until then.
    readonly property bool showing: player.hasVideo && player.position > 0
    function toggle() { if (playing) player.pause(); else player.play() }

    MediaPlayer {
        id: player
        source: vp.source
        videoOutput: out
        audioOutput: AudioOutput {}
        // Back to the first frame, stopped, when it runs out: the button offers play again.
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.EndOfMedia) { pause(); position = 0 }
        Component.onCompleted: play()
    }
    VideoOutput { id: out; anchors.fill: parent; fillMode: VideoOutput.PreserveAspectFit }
}
