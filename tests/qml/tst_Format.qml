import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

TestCase {
    name: "Format"
    when: windowShown
    visible: true
    function test_bytes() {
        compare(Kiki.Format.bytes(0), "0 B")
        compare(Kiki.Format.bytes(1126), "1.1 KB")
        compare(Kiki.Format.bytes(84 * 1024 * 1024 + 512 * 1024), "84.5 MB")
        compare(Kiki.Format.bytes(2.3 * 1024 * 1024 * 1024), "2.3 GB")
    }
    function test_friendly_dates() {
        const now = Date.now()
        compare(Kiki.Format.friendlyDate(now - 10 * 1000), "just now")
        compare(Kiki.Format.friendlyDate(now - 12 * 60 * 1000), "12 min ago")
        const y = new Date(now); y.setDate(y.getDate() - 1); y.setHours(14, 2, 0, 0)
        compare(Kiki.Format.friendlyDate(y.getTime()), "yesterday 14:02")
        const old = new Date(2024, 8, 12, 9, 30).getTime()
        compare(Kiki.Format.friendlyDate(old), "12 Sep 2024")
        compare(Kiki.Format.friendlyDate(0), "")
        compare(Kiki.Format.date(old), "12 Sep 2024 09:30")
    }
    function test_relative_and_heat() {
        const now = Date.now(), h = 3600000
        compare(Kiki.Format.relative(now - 10 * 1000), "just now")
        compare(Kiki.Format.relative(now - 5 * 60 * 1000), "5 min ago")
        compare(Kiki.Format.relative(now - 3 * h), "3 h ago")
        compare(Kiki.Format.relative(now - 2 * 24 * h), "2 days ago")
        compare(Kiki.Format.relative(now - 21 * 24 * h), "3 weeks ago")
        compare(Kiki.Format.relative(now - 150 * 24 * h), "5 months ago")
        compare(Kiki.Format.relative(now - 800 * 24 * h), "2 years ago")
        compare(Kiki.Format.relative(0), "—")
        const accent = Qt.rgba(0.48, 0.64, 0.97, 1)
        fuzzyCompare(Kiki.Format.heat(now - h, accent).a, 0.5, 0.01)
        fuzzyCompare(Kiki.Format.heat(now - 24 * h, accent).a, 0.28, 0.03)
        fuzzyCompare(Kiki.Format.heat(now - 365 * 24 * h, accent).a, 0.05, 0.01)
        compare(Kiki.Format.heat(0, accent).a, 0)
    }
    function test_display_and_crumbs() {
        compare(Kiki.Format.display("file:///home/david/Projects/kiki", "/home/david"), "~/Projects/kiki")
        compare(Kiki.Format.display("file:///home/david", "/home/david"), "~")
        compare(Kiki.Format.display("sftp://homelab/srv/kiki", "/home/david"), "homelab/srv/kiki")
        compare(Kiki.Format.crumbs("file:///home/david/Projects/kiki", "/home/david"), ["~", "Projects", "kiki"])
        compare(Kiki.Format.crumbs("file:///", "/home/david"), ["/"])
    }
}
