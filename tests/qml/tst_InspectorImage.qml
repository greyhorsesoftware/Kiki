import QtQuick
import QtTest
import "../../qml/kiki/ui" as UI
import KikiTest

// An image in the info panel takes the panel's width and keeps its own ratio. Its height was bound
// to the picture's implicit size while the picture filled the box — "Binding loop detected for
// property height", on every image, in every session's log (plan 29 F).
TestCase {
    id: tc
    name: "InspectorImage"
    when: windowShown
    visible: true
    width: 420; height: 900

    UI.Inspector { id: insp; anchors.fill: parent; standalone: true }

    function init() { Wire.reset(); Wire.connectAll(); insp.uri = ""; insp.row = null }

    function show(name) {
        insp.row = ({ name: name, kind: "image", isDir: false, git: null, meta: { size: 10, mtime: 1700000000000, mode: 0o644 } })
        insp.uri = Qt.resolvedUrl("../../app-images/" + name).toString()
    }

    function test_an_image_preview_sizes_itself_without_a_binding_loop() {
        failOnWarning(/Binding loop/)
        show("kikifull.png")
        const box = findChild(insp, "inspector-preview")
        verify(box !== null)
        tryVerify(() => box.ratio > 0, 3000, "the picture loads and its ratio is known")
        // As wide as the panel lets it be, as tall as its ratio makes that — within the caps.
        const want = Math.min(400, Math.round((box.width - 12) * box.ratio) + 12, Math.round(insp.height * 0.4))
        compare(box.height, want)
    }

    function test_the_next_file_does_not_inherit_the_last_pictures_shape() {
        show("kikifull.png")
        const box = findChild(insp, "inspector-preview")
        tryVerify(() => box.ratio > 0, 3000)
        insp.row = ({ name: "notes.txt", kind: "text", isDir: false, git: null, meta: { size: 1, mtime: 1, mode: 0o644 } })
        insp.uri = "file:///home/t/notes.txt"
        compare(box.ratio, 0)
    }
}
