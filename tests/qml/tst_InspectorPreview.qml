import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI
import KikiTest

// What the info panel draws while the daemon is still working out a file's preview. "This file
// has no preview" is an answer; drawing the kind icon before it arrives made every file show its
// icon for a moment and then its preview.
TestCase {
    id: tc
    name: "InspectorPreview"
    when: windowShown
    visible: true
    width: 420; height: 900

    UI.Inspector { id: insp; anchors.fill: parent; standalone: true }
    SignalSpy { id: opened; target: insp; signalName: "open" }

    function init() { Wire.reset(); Wire.connectAll(); insp.uri = ""; insp.row = null }

    function show(name, kind, isDir) {
        insp.row = ({ name: name, kind: kind, isDir: !!isDir, git: null, meta: { size: 10, mtime: 1700000000000, mode: 0o644 } })
        insp.uri = "file:///home/t/" + name
    }

    function test_no_kind_icon_while_the_preview_is_still_coming() {
        show("notes.txt", "text")
        const icon = findChild(insp, "inspector-kind-icon")
        verify(insp.previewPending)
        verify(!icon.visible)
        Wire.replyTo("Preview", { text: "hello" })
        verify(!insp.previewPending)
        verify(!icon.visible)             // it has a preview: the icon never showed at all
    }

    function test_the_kind_icon_is_the_answer_when_there_is_no_preview() {
        show("blob.bin", "file")
        const icon = findChild(insp, "inspector-kind-icon")
        verify(!icon.visible)
        const asked = Wire.last("Preview")
        Wire.fail(asked.id, "Unsupported", "no preview for this kind")   // how the daemon says it
        verify(!insp.previewPending)
        verify(icon.visible)
    }

    // A folder is its icon, and its row already says it is a folder: nothing to wait for.
    function test_a_folder_shows_its_icon_at_once() {
        show("Projects", "folder", true)
        verify(insp.previewPending)
        verify(findChild(insp, "inspector-kind-icon").visible)
    }

    // A picture on this machine is drawn from the file itself, so it starts without the daemon.
    function test_a_local_picture_never_shows_the_kind_icon() {
        show("car.jpg", "image")
        verify(!findChild(insp, "inspector-kind-icon").visible)
        Wire.replyTo("Preview", { path: "/tmp/thumb.png" })
        verify(!findChild(insp, "inspector-kind-icon").visible)
    }

    // A video plays where its still is — but only once somebody asks. Moving the selection down
    // a folder of clips must not start a player for each.
    function test_a_video_plays_only_after_a_click() {
        show("clip.mp4", "video")
        Wire.replyTo("Preview", { path: "/tmp/still.png" })
        const player = findChild(insp, "inspector-video")
        const area = findChild(insp, "inspector-video-area")
        verify(area.visible)
        compare(player.status, Loader.Null)          // nothing built, QtMultimedia not loaded

        mouseClick(area, area.width / 2, area.height / 2)
        tryCompare(player, "status", Loader.Ready)
        compare(player.item.source.toString(), "file:///home/t/clip.mp4")

        // Another file: the player goes with the one it was playing.
        show("notes.txt", "text")
        compare(player.status, Loader.Null)
        verify(!area.visible)
    }

    // Sound, where it is, how long it is: the strip under a playing video (plan 29 F).
    function test_a_playing_video_can_be_muted_and_the_choice_outlives_it() {
        show("clip.mp4", "video")
        Wire.replyTo("Preview", { path: "/tmp/still.png" })
        const player = findChild(insp, "inspector-video")
        const area = findChild(insp, "inspector-video-area")
        verify(!findChild(insp, "inspector-video-strip").visible, "no strip until it is a player")
        mouseClick(area, area.width / 2, area.height / 2)
        tryCompare(player, "status", Loader.Ready)
        verify(!player.item.muted, "sound is on to begin with")

        const mute = findChild(insp, "inspector-video-mute")
        mouseMove(mute, mute.width / 2, mute.height / 2)
        tryVerify(() => findChild(insp, "inspector-video-strip").visible)
        wait(600)   // or this click reads as the back half of a double click on the still
        mouseClick(mute, mute.width / 2, mute.height / 2)
        verify(insp.videoMuted)
        verify(player.item.muted)

        // The next video starts the way this one was left.
        show("other.mp4", "video")
        Wire.replyTo("Preview", { path: "/tmp/still2.png" })
        mouseClick(area, area.width / 2, area.height / 2)
        tryCompare(player, "status", Loader.Ready)
        verify(player.item.muted)
        insp.videoMuted = false
    }

    function test_the_scrub_bar_seeks_to_where_it_is_pressed() {
        show("clip.mp4", "video")
        Wire.replyTo("Preview", { path: "/tmp/still.png" })
        const player = findChild(insp, "inspector-video")
        const area = findChild(insp, "inspector-video-area")
        mouseClick(area, area.width / 2, area.height / 2)
        tryCompare(player, "status", Loader.Ready)
        // No real file behind it, so no duration: a press must be harmless, not an exception.
        const scrub = findChild(insp, "inspector-video-scrub")
        mouseMove(scrub, 5, scrub.height / 2)
        mousePress(scrub, scrub.width / 2, scrub.height / 2)
        mouseRelease(scrub, scrub.width / 2, scrub.height / 2)
        compare(scrub.fraction, 0)
        compare(findChild(insp, "inspector-video-clock").text, "0:00 / 0:00")
    }

    function test_the_play_button_floats_in_only_under_the_pointer() {
        show("clip.mp4", "video")
        Wire.replyTo("Preview", { path: "/tmp/still.png" })
        const area = findChild(insp, "inspector-video-area")
        const button = findChild(insp, "inspector-video-button")
        mouseMove(insp, 5, insp.height - 5)
        verify(!button.visible)
        mouseMove(area, area.width / 2, area.height / 2)
        tryVerify(() => button.visible)
        // In the middle of the picture, both ways.
        const pair = findChild(insp, "inspector-video-buttons")
        const mid = pair.mapToItem(area, pair.width / 2, pair.height / 2)
        verify(Math.abs(mid.x - area.width / 2) <= 1 && Math.abs(mid.y - area.height / 2) <= 1, mid.x + "," + mid.y)
        mouseMove(insp, 5, insp.height - 5)
        tryVerify(() => !button.visible)
    }

    // Beside play: open the file in its own application, and stop ours so the two do not talk
    // over each other. It must not also count as a click on the still.
    function test_the_open_button_opens_the_file_without_starting_playback() {
        show("clip.mp4", "video")
        Wire.replyTo("Preview", { path: "/tmp/still.png" })
        const area = findChild(insp, "inspector-video-area")
        mouseMove(area, area.width / 2, area.height / 2)
        const open = findChild(insp, "inspector-video-open")
        tryVerify(() => open.visible)
        opened.clear()
        mouseMove(open, open.width / 2, open.height / 2)
        verify(open.visible)                       // still there with the pointer on it
        mouseClick(open, open.width / 2, open.height / 2)
        compare(opened.count, 1)
        compare(opened.signalArguments[0][0], "file:///home/t/clip.mp4")
        compare(findChild(insp, "inspector-video").status, Loader.Null)
    }

    // Pictures and text have nothing to play.
    function test_only_a_video_gets_the_play_area() {
        show("car.jpg", "image")
        Wire.replyTo("Preview", { path: "/tmp/thumb.png" })
        verify(!findChild(insp, "inspector-video-area").visible)
    }

    // The name sits level with the icon to its left.
    function test_the_name_is_centred_on_the_header_icon() {
        show("notes.txt", "text")
        const icon = findChild(insp, "inspector-title-icon")
        const title = findChild(insp, "inspector-title")
        verify(title.height < icon.height)          // otherwise this proves nothing
        const iconMid = icon.mapToItem(insp, 0, icon.height / 2).y
        const titleMid = title.mapToItem(insp, 0, title.height / 2).y
        verify(Math.abs(iconMid - titleMid) <= 1, "icon " + iconMid + " vs name " + titleMid)
    }

    // An answer for the file we have already left must not end the wait for the one we are on.
    function test_a_late_answer_for_another_file_changes_nothing() {
        show("a.txt", "text")
        const first = Wire.last("Preview")
        show("b.txt", "text")
        Wire.reply(first.id, { text: "stale" })
        verify(insp.previewPending)
        verify(insp.preview === null)
    }
}
