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

    // ---------------------------------------------------------------- a folder with no pictures

    function openTree(entries) {
        fake.tree = { "file:///home/t/docs": entries }
        pane.open("file:///home/t/docs")
        wait(60)
    }

    // Not an empty state: its folders and files, with the artwork Icon view draws, scaled up.
    function test_a_folder_with_no_pictures_shows_its_entries_large() {
        openTree([fake.dir("Projects"), fake.file("notes.txt"), fake.file("report.pdf", { kind: "pdf" })])
        tryCompare(gallery, "current", 0)
        const icon = findChild(gallery, "gallery-stage-icon")
        verify(icon.visible)
        compare(icon.kind, "folder")
        verify(icon.size >= 128 && icon.size <= 320, "scaled to the stage: " + icon.size)
        compare(icon.size, gallery.stageIconSize)
        compare(findChild(gallery, "gallery-stage-label").text, "Projects")

        verify(gallery.step(1))
        compare(icon.kind, "text")
        compare(findChild(gallery, "gallery-stage-label").text, "notes.txt")
        verify(gallery.step(1))
        compare(icon.kind, "pdf")
        verify(!gallery.step(1) || gallery.current === 2, "the last entry is the end")
    }

    function test_a_folder_with_nothing_in_it_says_so() {
        openTree([])
        tryCompare(findChild(gallery, "gallery-stage-label"), "text", "Empty folder")
        verify(!findChild(gallery, "gallery-stage-icon").visible)
    }

    // ---------------------------------------------------------------- deleting your way through

    // The row goes and the selection with it; the stage used to jump back to the first picture.
    function test_after_a_delete_the_stage_moves_to_what_followed() {
        const pics = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"].map(n => fake.file(n, { kind: "image" }))
        openTree(pics)
        pane.selection.set(1)                       // b.jpg
        gallery.keepPlace()
        fake.tree = { "file:///home/t/docs": pics.filter(p => p.name !== "b.jpg") }
        pane.selection.clear()
        fake._resetListing(pane.listing.lid)
        tryCompare(gallery, "current", 1)
        tryVerify(() => gallery.row && gallery.row.name === "c.jpg", 2000, "what followed b.jpg, not a.jpg")
    }

    function test_deleting_the_last_one_steps_back() {
        const pics = ["a.jpg", "b.jpg", "c.jpg"].map(n => fake.file(n, { kind: "image" }))
        openTree(pics)
        pane.selection.set(2)
        gallery.keepPlace()
        fake.tree = { "file:///home/t/docs": pics.slice(0, 2) }
        pane.selection.clear()
        fake._resetListing(pane.listing.lid)
        tryCompare(gallery, "current", 1)
    }

    /// What the filmstrip shows for a row: its thumbnail if it has one, else the kind artwork.
    function stripThumbs() {
        const strip = findChild(gallery, "gallery-strip")
        const out = []
        for (const shot of strip.contentItem.children) {
            if (shot.index === undefined || !shot.r) continue
            const img = shot.children.find(c => c.source !== undefined)
            out.push({ index: shot.index, name: shot.r.name, thumb: shot.r.thumb,
                       showing: img && img.visible ? String(img.source) : "kind-icon" })
        }
        return out.sort((a, b) => a.index - b.index)
    }

    // Plan 31, 4a: a rescan (a job finishing, a file arriving, `Refresh`) makes the daemon re-send
    // the listing. The thumbnails it already knew come back with it, and the strip must draw them
    // — this is the view's half of "pictures turn into generic file icons".
    function test_the_filmstrip_keeps_its_pictures_across_a_reset() {
        const pics = ["a.jpg", "b.jpg", "c.jpg"].map((n, i) => fake.file(n, { kind: "image", thumb: "/th/" + n + ".png" }))
        openTree(pics)
        tryVerify(() => stripThumbs().length === 3)
        compare(stripThumbs().map(s => s.showing), ["file:///th/a.jpg.png", "file:///th/b.jpg.png", "file:///th/c.jpg.png"])

        // The same rows again, as a rescan sends them: nothing about them changed.
        fake._resetListing(pane.listing.lid)
        tryVerify(() => stripThumbs().length === 3)
        compare(stripThumbs().map(s => s.showing), ["file:///th/a.jpg.png", "file:///th/b.jpg.png", "file:///th/c.jpg.png"],
                "still pictures, not the generic file artwork")
    }

    // And when a thumbnail lands late (the daemon pushes `Rows` when the job finishes), the tile
    // that was showing the kind icon takes the picture up without being scrolled away and back.
    function test_a_thumbnail_that_arrives_late_reaches_the_tile() {
        const pics = ["a.jpg", "b.jpg"].map(n => fake.file(n, { kind: "image" }))
        openTree(pics)
        tryVerify(() => stripThumbs().length === 2)
        compare(stripThumbs()[1].showing, "kind-icon")
        const withThumb = fake.file("b.jpg", { kind: "image", thumb: "/th/b.png" })
        fake.emitEvent({ event: "Rows", lid: pane.listing.lid, first: 1, rows: [withThumb] })
        tryVerify(() => stripThumbs()[1].showing === "file:///th/b.png", 2000, "the tile re-read its row")
    }

    // ---------------------------------------------------------------- the size it is decoded at

    /// A folder of real files on disk, so the pictures really decode: `tests/qml/fixtures` holds
    /// a 120 × 80 icon and a 2400 × 600 panorama, either side of what the stage asks for.
    function openFixtures(names) {
        const dir = Qt.resolvedUrl("fixtures").toString()
        fake.tree = { [dir]: names.map(n => fake.file(n, { kind: "image" })) }
        pane.open(dir)
        wait(60)
        tryCompare(gallery, "current", 0)
        tryVerify(() => gallery.front.status === Image.Ready && gallery.front.implicitWidth > 0, 5000, "the picture loads")
        return gallery.front
    }

    // `sourceSize` is meant as a ceiling on the decode. With `PreserveAspectFit` set on the frame
    // Qt read it as a size to scale TO instead, and a 120 × 80 icon came back 1600 px wide: shown
    // stage-sized rather than at its own size, and `1` showing the upscale, not the file.
    function test_a_small_picture_is_decoded_at_its_own_size() {
        const f = openFixtures(["small.png"])
        compare(f.implicitWidth, 120)
        compare(f.implicitHeight, 80)
        verify(f.sourceSize.width > 120, "the cap was asked for: " + f.sourceSize.width)
    }

    function test_a_small_picture_is_shown_at_its_own_size() {
        const f = openFixtures(["small.png"])
        compare(gallery.fitScale, 1, "nothing to shrink, and never blown up")
        compare(f.width, 120)
        compare(f.height, 80)
    }

    function test_actual_size_is_the_files_own_pixels() {
        const f = openFixtures(["small.png"])
        gallery.actual()
        compare(f.width, 120)
        compare(f.height, 80)
    }

    // The other half of the rule: a picture larger than the stage needs is still decoded smaller
    // than it is — capped, and in its own proportions, not stretched to the cap.
    function test_a_big_picture_is_still_decoded_capped() {
        const f = openFixtures(["wide.png"])                    // 2400 × 600
        verify(f.implicitWidth < 2400, "smaller than the file, not larger: " + f.implicitWidth)
        verify(f.implicitWidth <= f.sourceSize.width && f.implicitHeight <= f.sourceSize.height,
               "inside the cap: " + f.implicitWidth + "x" + f.implicitHeight)
        verify(Math.abs(f.implicitWidth / f.implicitHeight - 4) < 0.02, "and still four to one")
        verify(gallery.fitScale < 1, "so it is shrunk to the stage")
    }

    // ---------------------------------------------------------------- dragging out

    // The offscreen platform ends a drag the moment it starts (nothing is there to drop on), so
    // what is asserted is that one began and what it carried.
    Component { id: spyC; SignalSpy { signalName: "dragStarted" } }
    function spyOn(proxy) { return spyC.createObject(tc, { target: proxy.Drag }) }
    /// Press on `item` at (x, y), move by (dx, dy) in steps with the button held, let go.
    function pull(item, x, y, dx, dy) {
        mousePress(item, x, y, Qt.LeftButton)
        for (let k = 1; k <= 6; k++) mouseMove(item, x + dx * k / 6, y + dy * k / 6, -1, Qt.LeftButton)
        wait(50)                                   // an automatic drag starts from a queued event
        mouseRelease(item, x + dx, y + dy, Qt.LeftButton)
    }
    function stripTile(i) {
        return findChild(gallery, "gallery-strip").contentItem.children.find(s => s.index === i && s.r)
    }
    function pictures(names) { openTree(names.map(n => fake.file(n, { kind: "image" }))) }

    function test_the_picture_on_show_drags_out_as_a_uri_list() {
        pictures(["a.jpg", "b.jpg"])
        tryCompare(gallery, "current", 0)
        const proxy = findChild(gallery, "gallery-stage-drag")
        const spy = spyOn(proxy)
        pull(gallery, gallery.width / 2, 40, 0, 40)            // above the pill, on the picture
        compare(spy.count, 1, "a drag began")
        compare(proxy.Drag.mimeData["text/uri-list"], "file:///home/t/docs/a.jpg\r\n")
        spy.destroy()
    }

    function test_a_selection_drags_out_whole() {
        pictures(["a.jpg", "b.jpg", "c.jpg"])
        pane.selection.set(0)
        pane.selection.toggle(2)                               // a and c, with c on the stage
        const proxy = findChild(gallery, "gallery-stage-drag")
        pull(gallery, gallery.width / 2, 40, 0, 40)
        compare(proxy.Drag.mimeData["text/uri-list"], "file:///home/t/docs/a.jpg\r\nfile:///home/t/docs/c.jpg\r\n")
    }

    // A tile pulled out of the filmstrip carries its own picture, and the stage goes to it.
    function test_a_filmstrip_tile_drags_out_its_picture() {
        pictures(["a.jpg", "b.jpg"])
        tryVerify(() => stripTile(1))
        const tile = stripTile(1)
        const proxy = findChild(tile, "strip-drag")
        const spy = spyOn(proxy)
        pull(tile, tile.width / 2, tile.height / 2, 0, -60)   // up, out of the strip
        compare(spy.count, 1, "a drag began")
        compare(proxy.Drag.mimeData["text/uri-list"], "file:///home/t/docs/b.jpg\r\n")
        compare(gallery.current, 1)
        spy.destroy()
    }

    function test_a_tile_pulled_sideways_drags_too() {
        pictures(["a.jpg", "b.jpg", "c.jpg"])
        tryVerify(() => stripTile(0))
        const tile = stripTile(0)
        const spy = spyOn(findChild(tile, "strip-drag"))
        pull(tile, tile.width / 2, tile.height / 2, 60, 0)
        compare(spy.count, 1)
        spy.destroy()
    }

    // A picture zoomed past the stage is moved around by dragging it; that must not become a drag
    // out of the gallery. A real file, because only a decoded picture has a size to zoom.
    function test_a_zoomed_picture_pans_instead_of_leaving() {
        const dir = Qt.resolvedUrl("../../app-images").toString()
        fake.tree = { [dir]: [fake.file("kikifull.png", { kind: "image" })] }
        pane.open(dir)
        tryCompare(gallery, "current", 0)
        tryVerify(() => gallery.front.status === Image.Ready && gallery.front.implicitWidth > 0, 5000, "the picture loads")
        const proxy = findChild(gallery, "gallery-stage-drag")
        const mouse = findChild(gallery, "gallery-stage-mouse")
        verify(!gallery.canPan && mouse.drag.target === proxy, "fitted: a drag leaves")

        gallery.actual()                                       // over 1024 px on an 800 × 516 stage
        tryVerify(() => gallery.canPan)
        verify(mouse.drag.target === null)
        const stage = findChild(gallery, "gallery-stage")
        const spy = spyOn(proxy)
        pull(gallery, gallery.width / 4, 300, 0, -120)
        compare(spy.count, 0, "no drag out")
        verify(stage.contentY > 0, "it panned: " + stage.contentY)
        spy.destroy()
    }

    // Remote pictures have no thumbnails (owner, 2026-09-20: file:// only), so a tile shows the
    // kind artwork rather than a broken image, and the stage says which file it is.
    function test_a_remote_picture_shows_its_kind_artwork() {
        fake.tree = { "sftp://ghs/pics": [fake.file("far.jpg", { kind: "image" })] }
        pane.open("sftp://ghs/pics")
        wait(60)
        tryVerify(() => stripThumbs().length === 1)
        compare(stripThumbs()[0].showing, "kind-icon")
        const icon = findChild(gallery, "gallery-stage-icon")
        const label = findChild(gallery, "gallery-stage-label")
        verify(icon.visible)
        compare(label.text, "far.jpg")
    }

    // The strip recycles its tiles as it scrolls. A tile held its row as a value from the first
    // update on, so one handed another place went on showing the file of the place before: a
    // picture under another's name — and, in a folder of mixed kinds, a text file's icon where
    // a picture was. Seen by the owner; every other view already read its row again.
    function stripTiles() {
        const out = []
        function walk(it) { for (const c of it.children) { if (c.objectName === "strip-thumb") out.push(c.parent); else walk(c) } }
        walk(gallery)
        // On screen: the strip also keeps its current item alive wherever it has scrolled to.
        return out.filter(t => { const v = t.ListView.view; return t.visible && t.index >= 0 && t.x + t.width > v.contentX && t.x < v.contentX + v.width })
    }
    function wrongTiles() {
        return stripTiles().filter(t => { const want = pane.listing.row(t.index); return !t.r || !want || t.r.name !== want.name })
                           .map(t => t.index + " shows " + (t.r ? t.r.name : "nothing") + " wants " + ((pane.listing.row(t.index) || {}).name))
    }
    function test_a_recycled_tile_shows_the_file_at_its_new_place() {
        const rows = []
        for (let i = 0; i < 400; i++) rows.push(i % 2 ? fake.file("n" + i + ".txt") : fake.file("p" + i + ".jpg", { kind: "image", thumb: "/t/" + i + ".png" }))
        fake.tree = { "file:///home/long": rows }
        pane.open("file:///home/long")
        wait(100)
        const strip = stripTiles()[0].ListView.view
        const from = strip.contentX
        for (let x = from; x < from + 3000; x += 37) { strip.contentX = x; wait(4) }
        wait(200)
        verify(stripTiles().length > 5)
        compare(wrongTiles(), [])
        for (let x = strip.contentX; x > 0; x -= 53) { strip.contentX = x; wait(4) }
        strip.contentX = 0
        tryVerify(() => wrongTiles().length === 0, 2000, "back at the start: " + wrongTiles())
    }

    // ---------------------------------------------------------------- 2026-09-22: the stage dressed
    // Under the picture, the picture itself blurred; while a picture is slow, its thumbnail stands
    // in; the current tile comes forward and the strip glides to it.
    function test_the_backdrop_is_the_pictures_own_thumbnail() {
        fake.tree = { "file:///home/p": [fake.file("a.jpg", { kind: "image", thumb: Qt.resolvedUrl("fixtures/small.png").toString().replace("file://", "") }), fake.file("n.txt")] }
        pane.open("file:///home/p"); wait(50)
        const backdrop = findChild(gallery, "gallery-backdrop")
        pane.selection.set(0)
        tryVerify(() => backdrop.visible, 2000, "no backdrop under a picture with a thumbnail")
        compare(backdrop.width, findChild(gallery, "gallery-stage").width, "it fills the stage")
        pane.selection.set(1)
        tryVerify(() => !backdrop.visible, 2000, "a text file has no backdrop")
    }
    function test_a_slow_picture_shows_its_thumbnail_first() {
        const thumb = Qt.resolvedUrl("fixtures/small.png").toString().replace("file://", "")
        fake.tree = { "file:///home/p": [fake.file("never.jpg", { kind: "image", thumb: thumb })] }
        pane.open("file:///home/p"); wait(50)
        pane.selection.set(0)                       // the file itself does not exist: it never arrives
        const preview = findChild(gallery, "gallery-preview")
        tryVerify(() => preview.opacity === 1, 2000, "nothing on the stage yet: the thumbnail should be up at once")
        verify(String(preview.source).endsWith("small.png"))
    }
    function test_the_current_tile_comes_forward_and_the_strip_glides() {
        const rows = []
        for (let i = 0; i < 60; i++) rows.push(fake.file("p" + i + ".jpg", { kind: "image", thumb: "/t/" + i + ".png" }))
        fake.tree = { "file:///home/many": rows }
        pane.open("file:///home/many"); wait(80)
        pane.selection.set(0); wait(200)
        const tile = t => stripTiles().find(x => x.index === t)
        tryVerify(() => tile(0) && tile(0).scale > 1.05, 1000, "the current tile did not grow")
        verify(tile(1).scale === 1 && tile(1).opacity < 1, "its neighbour stepped back")
        const strip = findChild(gallery, "gallery-strip")
        pane.selection.set(40); gallery.ensureVisible(40)      // as the window does after a key
        wait(30)
        const midway = strip.contentX
        verify(midway > 0 && midway < 40 * gallery.shotPitch - strip.width / 2, "the strip jumped rather than glided: " + midway)
        tryVerify(() => Math.abs(strip.contentX - (40 * gallery.shotPitch - (strip.width - gallery.shotWidth) / 2)) < 1, 1000, "and it ends centred on the tile")
    }
}
