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
    function test_display_and_crumbs() {
        compare(Kiki.Format.display("file:///home/david/Projects/kiki", "/home/david"), "~/Projects/kiki")
        compare(Kiki.Format.display("file:///home/david", "/home/david"), "~")
        compare(Kiki.Format.display("sftp://homelab/srv/kiki", "/home/david"), "homelab/srv/kiki")
        compare(Kiki.Format.crumbs("file:///home/david/Projects/kiki", "/home/david"), ["~", "Projects", "kiki"])
        compare(Kiki.Format.crumbs("file:///", "/home/david"), ["/"])
    }
}
