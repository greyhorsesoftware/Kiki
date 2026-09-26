import QtQuick
import QtMultimedia
import ".." as Kiki

// A video in the Quick Look window (docs/0.2.0/05-quicklook.md, decision 3): QtMultimedia's
// player on the FFmpeg backend, playing the moment it exists, sound on the desktop's default
// output. `Space` is taken — it closes the window — so `k` is play/pause as in mpv, with a
// click on the picture doing the same; `←`/`→` wind 5 s, 30 s with Shift; `m` mutes. The bar
// under the picture is the info panel's strip grown up: play/pause, the scrub bar, the time,
// mute — plain rectangles and a MouseArea, as that one is, not Controls.
Item {
    id: video
    /// The file, a local `file://` uri.
    property string uri: ""
    property string name: ""
    property var row: null
    focus: true

    readonly property bool playing: player.playbackState === MediaPlayer.PlayingState
    property bool muted: false
    readonly property int position: player.position
    readonly property int duration: player.duration
    readonly property bool hasAudio: player.hasAudio
    /// What the player said when it could not play the file; "" while it can.
    property string error: ""

    function toggle() { if (playing) player.pause(); else player.play() }
    /// `to` is milliseconds; anything outside the video is brought inside it.
    function seek(to) { if (player.seekable) player.position = Math.max(0, Math.min(player.duration, Math.round(to))) }
    function seekBy(ms) { seek(player.position + ms) }
    /// A length of time as the bar shows it: 0:07, 3:40, 1:02:05 — the info panel's clock, so
    /// the two players never disagree about the same file.
    function formatTime(ms) { return Kiki.Format.clock(ms) }

    MediaPlayer {
        id: player
        source: video.uri
        videoOutput: out
        audioOutput: AudioOutput { muted: video.muted }
        // Back to the first frame, paused, when it runs out: the button offers play again.
        onMediaStatusChanged: if (mediaStatus === MediaPlayer.EndOfMedia) { pause(); position = 0 }
        // Played once there is something to play. The window's loader may hand over `uri`
        // before or after this exists, and `play()` on a player with nothing in it is forgotten.
        onSourceChanged: if (source.toString() !== "") { video.error = ""; play() }
        Component.onCompleted: if (source.toString() !== "") play()
        onErrorOccurred: (err, what) => { if (err !== MediaPlayer.NoError) video.error = what }
    }
    // The compositor keeps a window's last frame around for a moment after it goes; a player
    // still running into it is sound with no picture.
    Component.onDestruction: player.stop()

    Keys.onPressed: event => {
        if (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.MetaModifier)) return
        const far = event.modifiers & Qt.ShiftModifier
        switch (event.key) {
        case Qt.Key_K: toggle(); break
        case Qt.Key_M: muted = !muted; break
        case Qt.Key_Left: seekBy(far ? -30000 : -5000); break
        case Qt.Key_Right: seekBy(far ? 30000 : 5000); break
        default: return                              // the window's keys — step, close — pass on
        }
        event.accepted = true
    }

    Rectangle { anchors.fill: parent; color: "black" }
    VideoOutput {
        id: out
        objectName: "quicklook-video-output"
        anchors.fill: parent; anchors.bottomMargin: bar.height
        fillMode: VideoOutput.PreserveAspectFit
    }
    MouseArea {
        objectName: "quicklook-video-picture"
        anchors.fill: out
        onClicked: video.toggle()
    }
    // The player's own words when it cannot play the file: a codec it has not got, a file
    // that is not a video after all.
    Text {
        objectName: "quicklook-video-error"
        visible: video.error !== ""
        anchors.centerIn: out
        width: Math.max(1, out.width - 48)
        horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
        text: video.error
        color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
    }

    Rectangle {
        id: bar
        objectName: "quicklook-video-bar"
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        height: 40; color: Qt.rgba(0, 0, 0, 0.75)
        Rectangle { width: parent.width; height: 1; color: Qt.rgba(1, 1, 1, 0.12) }
        Row {
            anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 12; spacing: 10
            Item {
                objectName: "quicklook-video-play"
                width: 28; height: parent.height
                Icon { anchors.centerIn: parent; name: video.playing ? "pause" : "play"; size: 20; color: "white" }
                Tip { objectName: "quicklook-video-play-tip"; visible: playArea.containsMouse; text: video.playing ? Kiki.T.tr("quicklook.video.pause") : Kiki.T.tr("quicklook.video.play") }
                MouseArea { id: playArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: video.toggle() }
            }
            Item {
                objectName: "quicklook-video-scrub"
                width: parent.width - 28 - clockText.width - 28 - 3 * parent.spacing; height: parent.height
                readonly property real fraction: video.duration > 0 ? video.position / video.duration : 0
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: parent.width; height: 4; radius: 2; color: Qt.rgba(1, 1, 1, 0.25) }
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: Math.round(parent.width * parent.fraction); height: 4; radius: 2; color: Kiki.Theme.accent }
                Rectangle { visible: scrubArea.containsMouse || scrubArea.pressed; anchors.verticalCenter: parent.verticalCenter; x: Math.round(parent.width * parent.fraction) - 6; width: 12; height: 12; radius: 6; color: "white" }
                MouseArea {
                    id: scrubArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; preventStealing: true
                    function go(x) { if (video.duration > 0) video.seek(video.duration * Math.max(0, Math.min(1, x / width))) }
                    onPressed: mouse => go(mouse.x)
                    onPositionChanged: mouse => { if (pressed) go(mouse.x) }
                }
            }
            Text {
                id: clockText
                objectName: "quicklook-video-clock"
                anchors.verticalCenter: parent.verticalCenter
                text: Kiki.T.tr("quicklook.video.time", { position: video.formatTime(video.position), duration: video.formatTime(video.duration) })
                color: "white"; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
            Item {
                objectName: "quicklook-video-mute"
                width: 28; height: parent.height
                Icon { anchors.centerIn: parent; name: video.muted ? "volume-off" : "volume"; size: 16; color: video.hasAudio || video.duration === 0 ? "white" : Qt.rgba(1, 1, 1, 0.35) }
                Tip {
                    objectName: "quicklook-video-mute-tip"
                    visible: muteArea.containsMouse
                    text: video.duration > 0 && !video.hasAudio ? Kiki.T.tr("quicklook.video.noSound")
                        : (video.muted ? Kiki.T.tr("quicklook.video.unmute") : Kiki.T.tr("quicklook.video.mute"))
                }
                // A click soon after another arrives as the second half of a double click, which a
                // MouseArea reports instead of `clicked`: it counts too.
                MouseArea { id: muteArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: video.muted = !video.muted; onDoubleClicked: video.muted = !video.muted }
            }
        }
    }
}
