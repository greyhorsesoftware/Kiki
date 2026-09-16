import QtQuick
import QtTest
import "../../qml/kiki" as Kiki

TestCase {
    name: "Format"
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
    function test_display_and_crumbs() {
        compare(Kiki.Format.display("file:///home/david/Projects/kiki", "/home/david"), "~/Projects/kiki")
        compare(Kiki.Format.display("file:///home/david", "/home/david"), "~")
        compare(Kiki.Format.display("sftp://homelab/srv/kiki", "/home/david"), "homelab/srv/kiki")
        compare(Kiki.Format.crumbs("file:///home/david/Projects/kiki", "/home/david"), ["~", "Projects", "kiki"])
        compare(Kiki.Format.crumbs("file:///", "/home/david"), ["/"])
    }
}
