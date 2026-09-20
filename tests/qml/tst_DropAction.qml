import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// What a drop does, for every pair of ends: this machine, an SFTP server, an FTPS server.
// A drop is a job — `Submit { op: { op, items, dest } }` — so that is what is asserted.
TestCase {
    id: tc
    name: "DropAction"

    Kiki.Pane { id: pane }

    readonly property string local: "file:///home/t/site"
    readonly property string local2: "file:///home/t/backup"
    readonly property string sftp: "sftp://nas/srv/site"
    readonly property string sftp2: "sftp://nas/srv/old"
    readonly property string sftpOther: "sftp://backup/srv/site"
    readonly property string ftps: "ftps://host/www"
    readonly property string ftps2: "ftps://host/www/old"

    function drop(urls, modifiers) {
        return { accepted: false, action: 0, hasUrls: true, hasText: false, urls: urls, modifiers: modifiers || 0, proposedAction: Qt.MoveAction,
                 accept: function (a) { this.accepted = true; this.action = a } }
    }
    function submitted() { const r = Wire.last("Submit"); return r ? r.op : null }
    function init() { Wire.reset() }

    // ---- the default, by where the two ends are
    function test_the_default_data() {
        return [
            { tag: "local -> local", from: local, to: local2, op: "move" },
            { tag: "local -> sftp", from: local, to: sftp, op: "copy" },
            { tag: "sftp -> local", from: sftp, to: local, op: "copy" },
            { tag: "local -> ftps", from: local, to: ftps, op: "copy" },
            { tag: "ftps -> local", from: ftps, to: local, op: "copy" },
            { tag: "sftp -> sftp, same server", from: sftp, to: sftp2, op: "move" },
            { tag: "ftps -> ftps, same server", from: ftps, to: ftps2, op: "move" },
            { tag: "sftp -> sftp, ANOTHER server", from: sftp, to: sftpOther, op: "copy" },
            { tag: "sftp -> ftps", from: sftp, to: ftps, op: "copy" },
            { tag: "ftps -> sftp", from: ftps, to: sftp, op: "copy" },
        ]
    }
    function test_the_default(d) {
        const items = [d.from + "/index.html", d.from + "/images"]
        const ev = drop(items)
        pane.dropInto(d.to, ev)
        const op = submitted()
        verify(op !== null, "a job was made")
        compare(op.op, d.op)
        compare(op.items, items, "files and folders alike, as they were dragged")
        compare(op.dest, d.to)
        verify(ev.accepted)
        compare(ev.action, d.op === "copy" ? Qt.CopyAction : Qt.MoveAction, "and the drag is told the same")
    }

    // ---- Ctrl copies and Shift moves, whatever the ends
    function test_modifiers_data() {
        const pairs = [["local -> local", local, local2], ["local -> sftp", local, sftp], ["sftp -> local", sftp, local], ["local -> ftps", local, ftps],
                       ["ftps -> local", ftps, local], ["sftp -> sftp", sftp, sftp2], ["ftps -> ftps", ftps, ftps2], ["sftp -> ftps", sftp, ftps], ["ftps -> sftp", ftps, sftp]]
        const rows = []
        for (const p of pairs) {
            rows.push({ tag: "Ctrl, " + p[0], from: p[1], to: p[2], mod: Qt.ControlModifier, op: "copy" })
            rows.push({ tag: "Shift, " + p[0], from: p[1], to: p[2], mod: Qt.ShiftModifier, op: "move" })
        }
        return rows
    }
    function test_modifiers(d) {
        pane.dropInto(d.to, drop([d.from + "/a.txt"], d.mod))
        compare(submitted().op, d.op)
    }

    // ---- what is not a drop at all
    function test_nothing_happens_data() {
        return [
            { tag: "onto the folder it is already in", urls: [local + "/a.txt"], to: local },
            { tag: "…written with a trailing slash", urls: [local + "/a.txt"], to: local + "/" },
            { tag: "a folder onto itself", urls: [local + "/images"], to: local + "/images" },
            { tag: "a folder into a folder inside it", urls: [local + "/images"], to: local + "/images/2026/may" },
            { tag: "the same on a server", urls: [sftp + "/images"], to: sftp + "/images/old" },
            { tag: "nothing dragged", urls: [], to: local2 },
        ]
    }
    function test_nothing_happens(d) {
        const ev = drop(d.urls)
        pane.dropInto(d.to, ev)
        compare(submitted(), null)
        verify(!ev.accepted, "so the drag springs back")
    }
    function test_a_folder_whose_name_begins_the_same_is_another_folder() {
        pane.dropInto(local + "/images-old", drop([local + "/images"]))
        compare(submitted().op, "move", "images-old is not inside images")
    }
    function test_only_what_is_not_already_there_goes() {
        pane.dropInto(local2, drop([local2 + "/here.txt", local + "/new.txt"]))
        compare(submitted().items, [local + "/new.txt"])
    }

    // ---- targets lie over each other, and Qt hands the drop to each in turn
    function test_a_drop_is_dealt_with_once() {
        const ev = drop([local + "/a.txt"])
        pane.dropInto(local2 + "/sub", ev)          // the folder row, on top
        pane.dropInto(local2, ev)                   // the view's background, underneath
        compare(Wire.count("Submit"), 1)
        compare(submitted().dest, local2 + "/sub")
    }
    function test_text_uri_list_is_read_when_there_are_no_urls() {
        const ev = drop([]); ev.hasUrls = false; ev.hasText = true; ev.text = "# comment\r\n" + sftp + "/a.txt\r\n" + sftp + "/b.txt\r\n"
        pane.dropInto(local, ev)
        compare(submitted().items, [sftp + "/a.txt", sftp + "/b.txt"])
        compare(submitted().op, "copy")
    }
}
