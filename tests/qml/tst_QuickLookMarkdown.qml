import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import "../../qml/kiki/ui/markdown_toc.js" as Toc
import KikiTest

// Quick Look's Markdown view (docs/0.2.0/05-quicklook.md): the heading parser on its own, then
// the view against a fake daemon — the column lists the headings, folds and remembers it, a
// click jumps the document, and the cut line says when the daemon stopped reading.
TestCase {
    id: tc
    name: "QuickLookMarkdown"
    when: windowShown
    visible: true
    width: 900; height: 400

    property var fake: null
    Component { id: fakeC; FakeDaemon {} }
    UI.QuickLookMarkdown { id: md; anchors.fill: parent }

    function init() {
        Kiki.T.language = "en"
        fake = fakeC.createObject(tc)
        md.daemon = fake
        md.uri = ""
        Kiki.Settings.set("view", "quickLookToc", false)
        Wire.reset()
    }
    function cleanup() { md.uri = ""; md.daemon = null; fake.destroy() }

    /// A document long enough to scroll: every heading followed by a page of prose.
    function longDoc() {
        const para = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor.\n\n".repeat(8)
        // A long tail after the last heading, so that it can be scrolled to the top with room to
        // spare: with one paragraph's worth the last heading was reachable by a single pixel, and
        // a pixel of layout under a loaded machine made "current" the heading before it.
        return "# Title\n\n" + para + "## First part\n\n" + para + "### Detail\n\n" + para + "## Second part\n\n" + para + "## First part\n\n" + para + para
    }
    function show(name, text, opts) {
        const o = opts || {}
        fake.texts["file:///home/t/" + name] = { text: text, bytes: o.bytes, truncated: o.truncated }
        md.uri = "file:///home/t/" + name
        md.name = name
    }
    function rows() { const r = []; for (let i = 0; ; i++) { const x = findChild(md, "ql-md-toc-row-" + i); if (!x) return r; r.push(x) } }
    function flick() { return findChild(md, "ql-md-flick") }
    function toc() { return findChild(md, "ql-md-toc") }

    // ------------------------------------------------------------ the parser

    function test_atx_levels_closing_hashes_and_trimming() {
        const h = Toc.headings("# One\n\n##   Two  \n\n###### Six ###\n\n####### seven hashes is text\n\n#NoSpace\n\n    # indented code\n\n# C# ##\n\n#\n")
        compare(h.length, 4)
        compare(h[0], { level: 1, text: "One", line: 0 })
        compare(h[1], { level: 2, text: "Two", line: 2 })
        compare(h[2], { level: 6, text: "Six", line: 4 })
        compare(h[3], { level: 1, text: "C#", line: 12 })    // a hash without a space before it is part of the text
    }

    function test_setext_underlines_and_the_rule_after_a_blank_line() {
        const h = Toc.headings("Title\n=====\n\nSub\n---\n\nprose\n\n---\n\nmore prose\n- item\n---\n> quote\n===\n# atx\n---\n")
        compare(h.map(x => x.level + ":" + x.text), ["1:Title", "2:Sub", "1:atx"])
        compare(h[1].line, 3)
    }

    function test_headings_inside_fences_are_ignored() {
        const h = Toc.headings("# Real\n\n```\n# not a heading\nSetext\n===\n```\n\n~~~md\n## nor this\n~~~\n\n## After\n\n````\n```\n# still inside the four-tick fence\n```\n````\n\n### Last\n")
        compare(h.map(x => x.level + ":" + x.text), ["1:Real", "2:After", "3:Last"])
    }

    function test_front_matter_is_not_a_setext_heading() {
        const h = Toc.headings("---\ntitle: Notes\n---\n\n# Notes\n")
        compare(h.map(x => x.text), ["Notes"])
    }

    function test_plain_strips_inline_marks_as_the_page_will() {
        compare(Toc.plain("**Bold** and *em* with `code` and [a link](http://x) ![pic](p.png)"), "Bold and em with code and a link pic")
        compare(Toc.plain("2 \\* 3"), "2 * 3")
    }

    // ------------------------------------------------------------ the view

    function test_a_document_without_headings_has_no_column() {
        show("plain.md", "Just a paragraph.\n\nAnd another.\n")
        tryCompare(md, "text", "Just a paragraph.\n\nAnd another.\n")
        verify(!toc().visible)
        compare(flick().anchors.left, md.left)
        compare(fake.count("ReadText"), 1)
        compare(fake.last("ReadText").fields.uri, "file:///home/t/plain.md")
    }

    function test_the_column_lists_the_headings_indented_by_level() {
        show("doc.md", longDoc())
        tryVerify(() => rows().length === 5)
        verify(toc().visible)
        const r = rows()
        compare(findChild(r[0], "label").text, "Title")
        compare(findChild(r[1], "label").text, "First part")
        compare(findChild(r[2], "label").text, "Detail")
        verify(findChild(r[0], "label").x < findChild(r[1], "label").x)
        verify(findChild(r[1], "label").x < findChild(r[2], "label").x)
        // Rendered, not shown as source: a heading line has no hash in it.
        const doc = findChild(md, "ql-md-doc")
        verify(doc.getText(0, doc.length).indexOf("# Title") < 0)
        verify(doc.getText(0, doc.length).indexOf("Title") === 0)
        compare(doc.baseUrl.toString(), "file:///home/t/")
        // The first heading is the one at the top of the view.
        tryCompare(md, "current", 0)
    }

    function test_the_column_is_as_wide_as_its_longest_heading_and_no_wider_than_a_third() {
        show("short.md", "# Hi\n\nx\n")
        tryVerify(() => rows().length === 1)
        const narrow = toc().width
        show("long.md", "# A heading whose words run on and on\n\nx\n")
        tryVerify(() => rows().length === 1 && toc().width !== narrow)
        verify(toc().width > narrow, "measured from the heading: " + toc().width + " vs " + narrow)
        show("wider.md", "# " + "word ".repeat(40) + "\n\nx\n")
        tryVerify(() => rows().length === 1 && toc().width === Math.floor(md.width / 3))
    }

    function test_the_chevron_folds_the_column_and_the_setting_remembers_it() {
        show("doc.md", longDoc())
        tryVerify(() => rows().length === 5)
        const wide = toc().width
        const fold = findChild(md, "ql-md-toc-fold")
        mouseClick(fold, fold.width / 2, fold.height / 2)
        verify(md.folded)
        verify(toc().width < wide && toc().width < 48, "a thin strip: " + toc().width)
        verify(!findChild(md, "ql-md-toc-title").visible)
        verify(!rows()[0].visible)
        let w = Wire.last("SetSettings")
        verify(w !== null, "the fold went to the daemon")
        compare(w.patch.view.quickLookToc, true)
        // `t` unfolds it again.
        md.forceActiveFocus()
        keyClick(Qt.Key_T)
        verify(!md.folded)
        compare(toc().width, wide)
        compare(Wire.last("SetSettings").patch.view.quickLookToc, false)
    }

    function test_the_fold_is_read_from_the_setting_for_the_next_document() {
        Kiki.Settings.set("view", "quickLookToc", true)
        show("doc.md", longDoc())
        tryVerify(() => rows().length === 5)
        verify(md.folded)
        verify(toc().width < 48)
    }

    function test_a_click_on_a_heading_scrolls_the_document_to_it() {
        show("doc.md", longDoc())
        // Laid out, not merely made: a Column places its rows a pass after the Repeater makes
        // them, and until then they all sit at y 0 with the last on top — a click meant for the
        // fourth row landed on the fifth whenever that pass came late (a loaded machine).
        tryVerify(() => rows().length === 5 && rows()[4].y > rows()[3].y && rows()[3].y > rows()[2].y)
        tryVerify(() => md._ys.length === 5 && md._ys[3] > 0)
        const f = flick()
        compare(f.contentY, 0)
        // The document is still growing for a moment after the text lands (longer on a loaded
        // machine): a jump asked for before the heading is reachable is clamped short of it.
        // Wait for the room, as a person's click comes after the page has drawn.
        tryVerify(() => f.contentHeight - f.height >= md._ys[3] - md.margin, 5000)
        const r = rows()[3]
        mouseClick(r, r.width / 2, r.height / 2)
        verify(f.contentY > md._ys[0], "moved past the first heading: " + f.contentY + " vs " + md._ys[0])
        verify(f.contentY >= md._ys[3] - md.margin - 1)
        compare(md.current, 3)
        // The same words twice: the later "First part" finds its own occurrence, after the earlier.
        verify(md._ys[4] > md._ys[1])
        const r4 = rows()[4]
        mouseClick(r4, r4.width / 2, r4.height / 2)
        verify(f.contentY > md._ys[3] - md.margin)
        compare(md.current, 4)
    }

    function test_the_keys_page_the_document() {
        show("doc.md", longDoc())
        tryVerify(() => rows().length === 5)
        const f = flick()
        md.forceActiveFocus()
        keyClick(Qt.Key_PageDown)
        verify(f.contentY > 0)
        const one = f.contentY
        keyClick(Qt.Key_End)
        compare(f.contentY, f.contentHeight - f.height)
        keyClick(Qt.Key_PageUp)
        verify(f.contentY < f.contentHeight - f.height)
        keyClick(Qt.Key_Home)
        compare(f.contentY, 0)
        keyClick(Qt.Key_PageDown)
        compare(f.contentY, one)
    }

    function test_the_cut_line_shows_when_the_daemon_stopped_reading() {
        show("big.md", "# Big\n\nsome of it\n", { bytes: 4 * 1024 * 1024, truncated: true })
        tryVerify(() => rows().length === 1)
        const cut = findChild(md, "ql-md-cut")
        verify(cut.visible)
        compare(cut.text, "cut at 4.0 MB")
        show("small.md", "# Small\n\nall of it\n")
        tryVerify(() => md.text.indexOf("Small") >= 0)
        verify(!cut.visible)
    }

    function test_a_file_that_cannot_be_read_shows_the_daemons_sentence() {
        md.uri = "file:///home/t/gone.md"
        tryCompare(md, "error", "no such file: file:///home/t/gone.md")
        verify(findChild(md, "ql-md-error").visible)
        verify(!toc().visible)
        show("doc.md", longDoc())
        tryVerify(() => rows().length === 5)
        compare(md.error, "")
    }

    // An answer for the file we have already left must not draw over the one we are on.
    function test_a_late_answer_for_another_file_changes_nothing() {
        fake.defer = true
        show("a.md", "# A\n")
        show("b.md", "# B\n")
        fake.flush()
        compare(md.text, "# B\n")
        compare(rows().length, 1)
        compare(findChild(rows()[0], "label").text, "B")
    }
}
