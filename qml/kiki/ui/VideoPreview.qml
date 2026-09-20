import QtQuick
import QtMultimedia

// A video playing inside the info panel's preview box. Built only once somebody clicks the still
// — until then the panel costs a video nothing, and QtMultimedia is never loaded. It starts
// playing as soon as it exists; `playing` and `toggle()` are what the play/pause button drives,
// and `muted`, `position`, `duration` and `seek()` what the strip under the picture does.
Item {
    id: vp
    property url source
    readonly property bool playing: player.playbackState === MediaPlayer.PlayingState
    /// True once a frame is on screen, so the still underneath can stay until then.
    readonly property bool showing: player.hasVideo && player.position > 0
    /// Sound is on unless the panel says otherwise; the choice is the panel's, so it outlives
    /// this player and the next video starts the way the last one was left.
    property bool muted: false
    readonly property int position: player.position
    readonly property int duration: player.duration
    readonly property bool hasAudio: player.hasAudio
    function toggle() { if (playing) player.pause(); else player.play() }
    /// `to` is milliseconds; anything outside the video is brought inside it.
    function seek(to) { if (player.seekable) player.position = Math.max(0, Math.min(player.duration, Math.round(to))) }

    MediaPlayer {
        id: player
        source: vp.source
        videoOutput: out
        audioOutput: AudioOutput { muted: vp.muted }
        // Back to the first frame, stopped, when it runs out: the button offers play again.
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.EndOfMedia) { pause(); position = 0 }
        Component.onCompleted: play()
    }
    VideoOutput { id: out; anchors.fill: parent; fillMode: VideoOutput.PreserveAspectFit }
}
