import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import KikiTest

// A listing cache that is destroyed lets go of its listing: the daemon is told, and the client
// no longer dispatches that listing's events to a dead object. Column view makes and destroys
// one for every folder walked into.
TestCase {
    name: "WindowCacheLifetime"
    Component { id: comp; Kiki.WindowCache {} }

    function test_destroying_a_cache_closes_its_listing() {
        Wire.reset()
        const c = comp.createObject(this)
        c.open("file:///tmp")
        const lid = c.lid
        verify(Kiki.Daemon._listings[lid] !== undefined)
        c.destroy()
        tryVerify(() => Wire.count("Close") === 1)
        compare(Wire.last("Close").lid, lid)
        compare(Kiki.Daemon._listings[lid], undefined)
    }
}
