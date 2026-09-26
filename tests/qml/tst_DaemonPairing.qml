import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/version.js" as Version

// A window and a daemon are a pair: the socket is named for the version, and a daemon of
// another version is refused on Hello with a sentence in the window's language.
TestCase {
    name: "DaemonPairing"

    function test_the_socket_is_named_for_the_version() {
        verify(Version.version.length > 0)
        verify(Kiki.Daemon.socketPath.endsWith("/kiki-" + Version.version + ".sock"), Kiki.Daemon.socketPath)
    }
    function test_a_daemon_of_another_version_is_refused_and_one_of_this_version_is_not() {
        compare(Kiki.Daemon.pairing({ kikid: Version.version, version: 1 }), "")
        const said = Kiki.Daemon.pairing({ kikid: "0.1.1", version: 1 })
        verify(said.indexOf("0.1.1") >= 0 && said.indexOf(Version.version) >= 0, said)
        compare(Kiki.Daemon.pairing({ version: 1 }), "", "a daemon too old to say is taken at its word")
        Kiki.T.language = "es"
        verify(Kiki.Daemon.pairing({ kikid: "0.1.1" }).indexOf("reinicia") >= 0, "in the window's language")
        Kiki.T.language = "en"
    }
}
