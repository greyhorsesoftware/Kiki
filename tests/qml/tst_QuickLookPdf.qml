import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The PDF face of Quick Look (docs/0.2.0/05-quicklook.md, decision 3) against the fake daemon:
// the page count is what `PdfInfo` said, a page is asked for only as it scrolls into view, the
// counter follows the scroll, a zoom asks again at the new width once the size has held, and a
// page the daemon could not draw shows its sentence in the page's place.
TestCase {
    id: tc
    name: "QuickLookPdf"
    when: windowShown
    visible: true
    width: 600; height: 600

    property var fake: null
    property var pdf: null
    Component { id: fakeC; FakeDaemon {} }
    Component { id: pdfC; UI.QuickLookPdf { anchors.fill: parent } }

    readonly property string doc: "file:///t/report.pdf"
    /// A real PNG for the pages, so the Image under each has something to decode.
    readonly property string png: Qt.resolvedUrl("fixtures/small.png").toString().replace(/^file:\/\//, "")

    function init() {
        Kiki.T.language = "en"
        fake = fakeC.createObject(tc)
        // US letter: 612 × 792 points, ten pages.
        fake.pdfs = ({ [doc]: { pages: 10, width: 612, height: 792, path: png, broken: [3], message: "page 3 could not be rendered" } })
    }
    function cleanup() {
        if (pdf) { pdf.destroy(); pdf = null }
        if (fake) { fake.destroy(); fake = null }
        Kiki.T.language = "en"
    }
    function open(uri) {
        pdf = pdfC.createObject(tc, { daemon: fake, uri: uri || doc })
        pdf.forceActiveFocus()
        return pdf
    }
    function pages() { return findChild(pdf, "quicklook-pdf-pages") }
    function askedPages() { return fake.requests("PdfPage").map(r => r.fields.page) }
    /// One page's height plus the gap after it, as the list lays them out.
    function pitch() { return Math.round(pdf.pageWidth * pdf.pageAspect) + pages().spacing }

    function test_the_count_and_the_shape_come_from_PdfInfo() {
        open()
        compare(fake.count("PdfInfo"), 1)
        compare(fake.last("PdfInfo").fields.uri, doc)
        compare(pdf.pages, 10)
        fuzzyCompare(pdf.pageAspect, 792 / 612, 0.001)
        compare(pages().count, 10)
        // Fitted: the window's width less the margins.
        compare(pdf.pageWidth, 600 - 2 * pdf.margin)
        compare(findChild(pdf, "quicklook-pdf-page").text, "Page 1 of 10")
    }

    function test_only_the_pages_in_view_are_asked_for() {
        open()
        // 552 wide, ~714 tall: page 1 fills the window and page 2 has not come into it.
        compare(askedPages(), [1])
        compare(fake.last("PdfPage").fields.width, pdf.pageWidth)
        compare(fake.last("PdfPage").fields.uri, doc)
        // Wait for the rest of the list to settle: nothing more is asked without a scroll.
        wait(50)
        compare(askedPages(), [1])
    }

    function test_scrolling_asks_for_the_pages_that_come_into_view_and_the_counter_follows() {
        open()
        const list = pages()
        list.contentY = 3 * pitch()
        list.forceLayout()
        tryCompare(pdf, "page", 4)
        compare(findChild(pdf, "quicklook-pdf-page").text, "Page 4 of 10")
        // Page 4 filled the window; page 5 has not shown; page 10 is nowhere near.
        verify(askedPages().indexOf(4) >= 0, "page 4 was not asked for: " + askedPages())
        verify(askedPages().indexOf(10) < 0, "page 10 was asked for before it was in view")
        // Halfway down page 4, the top of page 5 is in view.
        list.contentY = 3 * pitch() + pitch() / 2
        list.forceLayout()
        tryVerify(() => askedPages().indexOf(5) >= 0, 1000, "page 5 came into view and was not asked for")
    }

    function test_a_page_seen_once_is_not_asked_for_again() {
        open()
        const list = pages()
        list.contentY = 3 * pitch(); list.forceLayout()
        tryVerify(() => askedPages().indexOf(4) >= 0)
        const before = fake.count("PdfPage")
        list.contentY = 0; list.forceLayout()
        tryCompare(pdf, "page", 1)
        wait(50)
        compare(fake.count("PdfPage"), before)     // page 1's picture was kept
    }

    function test_plus_widens_the_page_and_asks_again_after_the_debounce() {
        open()
        const was = pdf.pageWidth
        keyClick(Qt.Key_Plus)
        verify(pdf.pageWidth > was, "the page did not widen")
        // The list is wider than the window now, but the daemon has not been asked yet.
        compare(fake.count("PdfPage"), 1)
        compare(pdf.askedWidth, was)
        wait(300)
        compare(pdf.askedWidth, pdf.pageWidth)
        compare(fake.count("PdfPage"), 2)
        compare(fake.last("PdfPage").fields.width, pdf.pageWidth)
        compare(fake.last("PdfPage").fields.page, 1)
        // `0` puts the width back; the picture at that width was kept, so nothing is asked.
        keyClick(Qt.Key_0)
        compare(pdf.pageWidth, was)
        wait(300)
        compare(pdf.askedWidth, was)
        compare(fake.count("PdfPage"), 2)
        compare(findChild(pdf, "quicklook-pdf-sheet-1").shown.width, was)
    }

    function test_minus_narrows_and_the_old_picture_stays_while_the_new_one_comes() {
        open()
        fake.defer = true
        keyClick(Qt.Key_Minus)
        wait(300)
        compare(fake.pending(), 1)
        // The sheet still shows the picture rendered at the old width, scaled down.
        const sheet = findChild(pdf, "quicklook-pdf-sheet-1")
        verify(sheet.shown && sheet.shown.width > pdf.pageWidth, "the old picture was dropped before the new one came")
        fake.flush()
        compare(sheet.shown.width, pdf.pageWidth)
    }

    function test_a_page_that_fails_shows_its_message_in_its_place() {
        open()
        const list = pages()
        list.contentY = 2 * pitch(); list.forceLayout()
        tryCompare(pdf, "page", 3)
        const sheet = findChild(pdf, "quicklook-pdf-sheet-3")
        verify(sheet, "no sheet for page 3")
        const said = findChild(sheet, "quicklook-pdf-sheet-error")
        tryVerify(() => said.visible)
        compare(said.text, "page 3 could not be rendered")
        // The page keeps its size, so the pages after it stay where they were.
        compare(sheet.height, Math.round(pdf.pageWidth * pdf.pageAspect))
    }

    function test_a_document_that_fails_shows_the_daemons_message() {
        open("file:///t/not-a.pdf")
        compare(pdf.pages, 0)
        const said = findChild(pdf, "quicklook-pdf-error")
        verify(said.visible)
        compare(said.text, "the file could not be read as a PDF")
        verify(!findChild(pdf, "quicklook-pdf-loading").visible)
    }

    function test_page_keys_scroll_and_home_and_end_go_to_the_ends() {
        open()
        const list = pages()
        keyClick(Qt.Key_PageDown)
        verify(list.contentY > 0, "PgDn did not scroll")
        keyClick(Qt.Key_End)
        tryCompare(pdf, "page", 10)
        keyClick(Qt.Key_Home)
        tryCompare(pdf, "page", 1)
        compare(list.contentY, pdf.topY)
    }

    function test_the_counter_speaks_the_language() {
        open()
        Kiki.T.language = "es"
        compare(findChild(pdf, "quicklook-pdf-page").text, "Página 1 de 10")
        Kiki.T.language = "ja"
        compare(findChild(pdf, "quicklook-pdf-page").text, "10ページ中 1ページ")
    }

    function test_another_file_starts_over() {
        open()
        fake.pdfs = Object.assign({}, fake.pdfs, { "file:///t/two.pdf": { pages: 2, width: 400, height: 400, path: png } })
        pdf.uri = "file:///t/two.pdf"
        compare(pdf.pages, 2)
        fuzzyCompare(pdf.pageAspect, 1, 0.001)
        compare(fake.last("PdfPage").fields.uri, "file:///t/two.pdf")
    }
}
