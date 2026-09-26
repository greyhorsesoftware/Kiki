import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The Quick Look window (docs/0.2.0/05-quicklook.md, L1 and L5) through its own API: `show`
// puts a file in it, the close box and the keys take it down, a kind the window cannot show
// gets the face with Open with…, a file on a server is fetched through the daemon and shown
// once its job is done — or offered instead when it is over the size limit — the size it was
// dragged to is remembered, and text says where it was cut. The shell's Space key and the
// following of the selection are the shell's and are driven by the e2e flow.
//
// Keys go through `handleKey`: the window is a window of its own, and a key sent from the
// test's window never reaches it (see tst_InfoPopover).
TestCase {
    id: tc
    name: "QuickLookWindow"
    when: windowShown
    visible: true
    width: 400; height: 300

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    UI.QuickLookWindow { id: ql; home: "/home"; paneUri: "file:///home/t" }
    SignalSpy { id: steps; target: ql; signalName: "step" }
    SignalSpy { id: openWiths; target: ql; signalName: "openWith" }
    SignalSpy { id: closes; target: ql; signalName: "dismissed" }

    readonly property int mb: 1024 * 1024
    property var wasView: null

    function initTestCase() {
        fake = fakeC.createObject(tc)
        fake.texts = {
            "file:///home/t/a.txt": { text: "hello world", bytes: 11, truncated: false },
            "file:///home/t/big.log": { text: "the first four megabytes", bytes: 4 * mb, truncated: true },
        }
        ql.daemon = fake
    }
    function init() {
        wasView = Kiki.Settings.view
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { quickLookSize: undefined, quickLookFetchLimit: undefined })
        Kiki.Jobs.list = []; Kiki.Jobs.changed()
        Wire.reset(); fake.reset(); fake._nextJob = 1
        steps.clear(); openWiths.clear(); closes.clear()
    }
    function cleanup() {
        ql.close()
        Kiki.Settings.view = wasView
        Kiki.Jobs.list = []; Kiki.Jobs.changed()
    }

    function local(name, opts) { return { row: fake.file(name, opts), uri: "file:///home/t/" + name } }
    function body() { return findChild(ql, "quicklook-body") }
    /// A copy job as kikid reports one, at some point along the way.
    function job(id, state, bytes, total) { return { id: id, op: "copy", state: state, done: 0, total: 1, bytes: bytes, bytesTotal: total, name: "photo.jpg", count: 1, hidden: true, direction: "download" } }
    function pushJob(j) { Kiki.Jobs.list = [j]; Kiki.Jobs.changed() }

    // ---------------------------------------------------------------- opening and closing
    function test_opens_with_the_name_as_its_title() {
        const f = local("a.txt")
        ql.show(f.row, f.uri)
        verify(ql.visible)
        // The name, then a suffix the compositor's rule matches whatever the language.
        compare(ql.title, "a.txt — Quick Look")
        verify(ql.title.endsWith(" — Quick Look"))
        compare(findChild(ql, "quicklook-name").text, "a.txt")
        compare(ql.face, "content")
        compare(ql.shown, "text")
        verify(!findChild(ql, "quicklook-where").visible, "in the pane's own folder: nothing to say about where")
    }
    function test_the_close_box_closes_it() {
        const f = local("a.txt")
        ql.show(f.row, f.uri)
        findChild(ql, "quicklook-close").clicked()
        verify(!ql.visible)
        compare(closes.count, 1)
    }
    function test_esc_and_space_close_it() {
        const f = local("a.txt")
        ql.show(f.row, f.uri)
        verify(ql.handleKey(Qt.Key_Escape, 0))
        verify(!ql.visible, "Esc")
        ql.show(f.row, f.uri)
        verify(ql.handleKey(Qt.Key_Space, 0))
        verify(!ql.visible, "Space again")
        compare(closes.count, 2)
    }
    function test_j_k_and_the_arrows_ask_the_shell_to_move_the_selection() {
        const f = local("a.txt")
        ql.show(f.row, f.uri)
        ql.handleKey(Qt.Key_J, 0); ql.handleKey(Qt.Key_K, 0)
        ql.handleKey(Qt.Key_Down, 0); ql.handleKey(Qt.Key_Right, 0); ql.handleKey(Qt.Key_Up, 0); ql.handleKey(Qt.Key_Left, 0)
        compare(steps.count, 6)
        compare(steps.signalArguments.map(a => a[0]), [1, -1, 1, 1, -1, -1])
        verify(ql.visible, "and the window stays: one window follows the selection")
        verify(!ql.handleKey(Qt.Key_A, 0), "a letter it has no meaning for is left to whoever is under it")
    }
    function test_show_again_replaces_what_is_in_it() {
        const a = local("a.txt"), z = local("z.zip", { kind: "archive" })
        ql.show(a.row, a.uri)
        ql.show(z.row, z.uri)
        compare(ql.title, "z.zip — Quick Look")
        compare(ql.face, "unsupported")
        compare(closes.count, 0, "the same window, not a close and an open")
    }
    function test_the_header_says_where_a_file_is_when_that_is_not_the_panes_folder() {
        ql.show(fake.file("x.txt"), "file:///home/u/x.txt")
        const where = findChild(ql, "quicklook-where")
        verify(where.visible)
        compare(where.text, "in ~/u")
        ql.show(fake.file("y.txt"), "sftp://nas/srv/y.txt")
        compare(where.text, "in nas/srv")
    }

    // ---------------------------------------------------------------- the unsupported face
    function test_a_kind_it_cannot_show_gets_its_face_and_open_with() {
        const z = local("z.zip", { kind: "archive", size: 2048 })
        ql.show(z.row, z.uri)
        compare(ql.face, "unsupported")
        compare(ql.shown, "other")
        verify(!body().active, "no component is loaded for it")
        verify(findChild(ql, "quicklook-unsupported").visible)
        compare(findChild(ql, "quicklook-kind-size").text, "Archive · 2.0 KB")
        findChild(ql, "quicklook-open-with").clicked()
        compare(openWiths.count, 1, "the button is the shell's own Open with menu")
    }
    function test_a_markdown_name_goes_to_markdown_whatever_its_kind() {
        compare(ql.componentFor("text", "README.md"), "QuickLookMarkdown.qml")
        compare(ql.componentFor("document", "notes.MARKDOWN"), "QuickLookMarkdown.qml")
        compare(ql.componentFor("code", "main.rs"), "QuickLookText.qml")
        compare(ql.componentFor("image", "a.jpg"), "QuickLookImage.qml")
        compare(ql.componentFor("archive", "a.zip"), "")
    }

    // ---------------------------------------------------------------- text
    // Code comes with the daemon's runs and is shown as rich text, a span per run in the
    // theme's colours; what is selected and copied is still the file's own characters — the
    // `<` in a string is escaped on the way in and back out.
    function test_code_is_coloured_from_the_daemons_runs_and_still_reads_as_itself() {
        const src = 'fn a() { "<b>" }'
        fake.texts["file:///home/t/main.rs"] = { text: src, language: "Rust", runs: [[0, 2, "keyword"], [9, 5, "string"]] }
        const f = local("main.rs", { kind: "code" })
        ql.show(f.row, f.uri)
        tryCompare(body(), "status", Loader.Ready)
        const edit = findChild(ql, "quicklook-text")
        tryCompare(edit, "textFormat", TextEdit.RichText)
        compare(edit.getText(0, edit.length), src)
        verify(edit.text.indexOf("color:" + Kiki.Theme.code.keyword) >= 0, "the keyword in the keyword colour: " + edit.text)
        verify(edit.text.indexOf("&lt;b&gt;") >= 0, "escaped on the way in")
        // Prose after code: plain again.
        const a = local("a.txt")
        ql.show(a.row, a.uri)
        tryCompare(findChild(ql, "quicklook-text"), "text", "hello world")
        compare(findChild(ql, "quicklook-text").textFormat, TextEdit.PlainText)
    }
    function test_text_shows_the_file_and_says_where_it_was_cut() {
        const a = local("a.txt")
        ql.show(a.row, a.uri)
        compare(fake.last("ReadText").fields.uri, a.uri)
        tryCompare(body(), "status", Loader.Ready)
        const edit = findChild(ql, "quicklook-text")
        compare(edit.text, "hello world")
        verify(edit.readOnly)
        verify(!findChild(ql, "quicklook-cut").visible, "whole: no cut line")
        const big = local("big.log", { kind: "text" })
        ql.show(big.row, big.uri)
        tryCompare(findChild(ql, "quicklook-text"), "text", "the first four megabytes")
        const cut = findChild(ql, "quicklook-cut")
        verify(cut.visible)
        compare(cut.children[1].text, "cut at 4 MB")
    }

    // ---------------------------------------------------------------- remote files
    function test_a_remote_file_is_fetched_and_shown_when_its_job_is_done() {
        ql.show(fake.file("photo.jpg", { kind: "image", size: 5 * mb }), "sftp://nas/pics/photo.jpg")
        const asked = fake.last("QuickLookFetch")
        verify(asked !== null, "asked the daemon for a copy")
        compare(asked.fields.uri, "sftp://nas/pics/photo.jpg")
        compare(ql.face, "fetching")
        compare(ql.job, 1)
        compare(ql.localUri, "", "nothing to show yet")
        verify(findChild(ql, "quicklook-fetching").visible)
        compare(findChild(ql, "quicklook-progress").text, "Fetching…", "no bytes known yet")
        pushJob(job(1, "running", 2 * mb, 5 * mb))
        compare(findChild(ql, "quicklook-progress").text, "Fetching… 2.0 MB of 5.0 MB")
        pushJob(job(1, "done", 5 * mb, 5 * mb))
        compare(ql.job, 0)
        compare(ql.localUri, "file:///cache/kiki/open/1/photo.jpg", "the copy, where the daemon said it would be")
        compare(ql.face, "content")
        compare(ql.shown, "image")
        tryCompare(body(), "status", Loader.Ready)
        verify(findChild(ql, "quicklook-picture") !== null, "the picture component, on the copy")
    }
    // A look leaves nothing behind: the copy is dropped when the window closes or moves on,
    // and a local file — never a copy — is never dropped.
    function test_a_fetched_copy_is_dropped_when_the_look_is_over() {
        ql.show(fake.file("photo.jpg", { kind: "image", size: 5 * mb }), "sftp://nas/pics/photo.jpg")
        pushJob(job(1, "done", 5 * mb, 5 * mb))
        compare(ql.localUri, "file:///cache/kiki/open/1/photo.jpg")
        verify(fake.last("QuickLookDrop") === null, "not while it is on show")
        ql.show(fake.file("other.jpg", { kind: "image", size: 5 * mb }), "sftp://nas/pics/other.jpg")
        compare(fake.last("QuickLookDrop").fields.path, "/cache/kiki/open/1/photo.jpg", "moving on drops the old copy")
        pushJob(job(2, "done", 5 * mb, 5 * mb))
        ql.close()
        compare(fake.last("QuickLookDrop").fields.path, "/cache/kiki/open/1/other.jpg", "closing drops the copy")
        compare(fake.count("QuickLookDrop"), 2)
        const a = local("a.txt")
        ql.show(a.row, a.uri)
        ql.close()
        compare(fake.count("QuickLookDrop"), 2, "a local file is not a copy")
    }
    function test_a_fetch_that_fails_is_said_in_the_daemons_words() {
        ql.show(fake.file("photo.jpg", { kind: "image", size: 5 * mb }), "sftp://nas/pics/photo.jpg")
        pushJob(Object.assign(job(1, "failed", 0, 5 * mb), { error: "Io: connection reset" }))
        compare(ql.face, "error")
        compare(findChild(ql, "quicklook-error-text").text, "connection reset")
    }
    function test_closing_while_fetching_cancels_the_job() {
        ql.show(fake.file("photo.jpg", { kind: "image", size: 5 * mb }), "sftp://nas/pics/photo.jpg")
        compare(ql.job, 1)
        ql.close()
        const c = Wire.last("Cancel")
        verify(c !== null, "the copy is stopped, not left running for nothing")
        compare(c.job, 1)
        compare(ql.job, 0)
    }
    function test_moving_on_while_fetching_cancels_the_old_fetch() {
        ql.show(fake.file("photo.jpg", { kind: "image", size: 5 * mb }), "sftp://nas/pics/photo.jpg")
        ql.show(fake.file("other.jpg", { kind: "image", size: 5 * mb }), "sftp://nas/pics/other.jpg")
        compare(Wire.last("Cancel").job, 1)
        compare(ql.job, 2)
        compare(fake.count("QuickLookFetch"), 2)
        // The first job finishing now is nobody's business.
        pushJob(job(1, "done", 5 * mb, 5 * mb))
        compare(ql.face, "fetching")
        compare(ql.localUri, "")
    }
    function test_a_file_over_the_limit_is_offered_not_fetched() {
        ql.show(fake.file("huge.iso", { kind: "archive", size: 500 * mb }), "sftp://nas/huge.iso")
        compare(fake.count("QuickLookFetch"), 0, "not fetched")
        compare(ql.face, "offer")
        verify(findChild(ql, "quicklook-offer").visible)
        compare(findChild(ql, "quicklook-too-big").text, "500 MB — fetch it?")
        findChild(ql, "quicklook-fetch-anyway").clicked()
        compare(fake.count("QuickLookFetch"), 1, "the button is the fetch")
        compare(ql.face, "fetching")
    }
    function test_the_limit_is_the_setting_in_megabytes() {
        Kiki.Settings.view = Object.assign({}, Kiki.Settings.view, { quickLookFetchLimit: 10 })
        ql.show(fake.file("a.bin", { kind: "file", size: 11 * mb }), "sftp://nas/a.bin")
        compare(ql.face, "offer")
        ql.show(fake.file("b.bin", { kind: "file", size: 9 * mb }), "sftp://nas/b.bin")
        compare(ql.face, "fetching")
        compare(fake.count("QuickLookFetch"), 1)
    }

    // ---------------------------------------------------------------- size
    function test_it_opens_at_the_default_within_the_screen_and_remembers_a_drag() {
        const f = local("a.txt")
        ql.show(f.row, f.uri)
        const cap = ql.screenCap()
        compare(ql.width, Math.min(900, cap.width))
        compare(ql.height, Math.min(650, cap.height))
        ql.close()
        compare(Wire.count("SetSettings"), 0, "left as it opened: nothing to remember")
        ql.show(f.row, f.uri)
        ql.width = 500; ql.height = 380                     // what a drag of the window's edge does
        ql.close()
        const p = Wire.last("SetSettings")
        verify(p !== null, "dragged: remembered")
        compare(p.patch.view.quickLookSize, { width: 500, height: 380 })
        // The stand-in window follows its implicit size by a binding the drag above broke, as
        // the real one follows it in C++; put it back so the next opening is measured.
        ql.width = Qt.binding(() => ql.implicitWidth); ql.height = Qt.binding(() => ql.implicitHeight)
        ql.show(f.row, f.uri)
        compare(ql.width, 500, "and that is what it opens at next")
        compare(ql.height, 380)
    }
    function test_a_picture_opens_no_bigger_than_it_is_once_per_opening() {
        const small = String(Qt.resolvedUrl("fixtures/small.png"))          // 120 × 80
        ql.show(fake.file("small.png", { kind: "image" }), small)
        compare(ql.shown, "image")
        tryCompare(body(), "status", Loader.Ready)
        tryVerify(() => body().item.natural.width === 120, 3000, "decoded")
        compare(body().item.natural.height, 80)
        // The picture plus its mount and the header, held to the least a window with a header can be.
        compare(ql.width, 320); compare(ql.height, 240)
        verify(ql.fitted)
        // Stepping on to a wide one keeps the window as it is: a window changing size at every j
        // is not something to look at.
        ql.show(fake.file("wide.png", { kind: "image" }), String(Qt.resolvedUrl("fixtures/wide.png")))
        tryVerify(() => body().item.natural.width === 2400, 3000)
        compare(ql.width, 320); compare(ql.height, 240)
        ql.close()
        compare(Wire.count("SetSettings"), 0, "the size it chose for the picture is not remembered as a drag")
    }
}
