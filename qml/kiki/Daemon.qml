pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "." as Kiki

// The one connection to kikid. Newline-delimited JSON over the Unix socket:
// requests carry an id and get one reply; events carry "event" and a lid.
// The shell never parses more than one window of rows per message.
Singleton {
    id: daemon

    property string socketPath: Quickshell.env("KIKI_SOCKET") || (Quickshell.env("XDG_RUNTIME_DIR") + "/kiki.sock")
    property bool ready: false
    property int protocolVersion: 0
    property string daemonVersion: ""

    property int _nextId: 1
    property var _pending: ({})      // id -> callback(ok, err)
    property var _listings: ({})     // lid -> object with handleEvent(msg)
    property int _nextLid: 1

    signal event(var msg)
    /// The connection came back after being lost: whatever was open needs opening again, since
    /// listing ids belong to the daemon that is gone.
    signal reconnected()

    function allocLid() { return _nextLid++ }

    // Send a request; cb(result, error) is called once with the reply.
    function request(type, fields, cb) {
        const id = _nextId++
        const msg = Object.assign({ id: id, type: type }, fields || {})
        if (!socket) { if (cb) cb(undefined, { code: "Disconnected", message: Kiki.T.tr("daemon.notConnected") }); return id }
        if (cb) _pending[id] = cb
        socket.write(JSON.stringify(msg) + "\n")
        socket.flush()
        return id
    }

    /// Answer everything still waiting with an error. A request whose daemon went away is never
    /// coming back, and a callback that never fires is a window that quietly stops working.
    function _failPending(code, message) {
        const pend = _pending
        _pending = ({})
        for (const id in pend) pend[id](undefined, { code: code, message: message })
    }
    function bind(lid, listing) { _listings[lid] = listing }
    function unbind(lid) { delete _listings[lid] }

    function _dispatch(line) {
        let msg
        try { msg = JSON.parse(line) } catch (e) { console.warn("kikid: bad json", line); return }
        if (msg.id !== undefined && (msg.ok !== undefined || msg.err !== undefined)) {
            // A numbered error is said in the window's language here, once, for every caller:
            // `message` becomes the sentence, `raw` keeps the daemon's English (0.2.0).
            if (msg.err && msg.err.n !== undefined) { msg.err.raw = msg.err.message; msg.err.message = Kiki.T.errorText(msg.err) }
            const cb = _pending[msg.id]
            if (cb) { delete _pending[msg.id]; cb(msg.ok, msg.err) }
            else if (msg.err) console.warn("kikid error", msg.err.code, msg.err.message)
            return
        }
        if (msg.event !== undefined) {
            // A listing that was destroyed without unbinding leaves a dead object here, and calling
            // into it is "handleEvent is not a function". WindowCache unbinds on destruction now;
            // this is so that the next thing to forget costs a dropped event and not an exception.
            const l = msg.lid !== undefined ? _listings[msg.lid] : null
            if (l && typeof l.handleEvent === "function") l.handleEvent(msg)
            else if (l) delete _listings[msg.lid]
            daemon.event(msg)
        }
    }

    /// True once a session has been established, so a later Hello is known to be a reconnect.
    property bool _hadSession: false

    /// kikid is socket-activated and restarts on failure; the window should follow it back up
    /// rather than sit there looking fine and doing nothing. A dropped `Socket` will not take a
    /// second connection — setting `connected` again does nothing — so the retry builds a new one.
    property Timer retry: Timer {
        interval: 700; repeat: true; running: false
        onTriggered: {
            if (daemon.ready) { stop(); return }
            // Connected but never greeted: the Hello went out into a socket that was closing.
            if (daemon.connected) { daemon._hello(); return }
            sock.active = false
            sock.active = true
        }
    }

    /// The handshake. Also the point where a second session is recognised as a reconnect.
    function _hello() {
        daemon.request("Hello", { version: 1, client: "kiki" }, (ok, err) => {
            if (!ok) { console.warn("kikid Hello failed", err && err.message); return }
            daemon.protocolVersion = ok.version
            daemon.daemonVersion = ok.daemon
            const again = daemon._hadSession
            daemon._hadSession = true
            daemon.ready = true
            daemon.retry.stop()
            if (again) daemon.reconnected()
        })
    }

    readonly property var socket: sock.item
    readonly property bool connected: sock.item ? sock.item.connected : false

    property Loader sock: Loader {
        active: true
        sourceComponent: Socket {
            path: daemon.socketPath
            connected: true
            parser: SplitParser {
                splitMarker: "\n"
                onRead: line => daemon._dispatch(line)
            }
            // The socket may already be up by the time this object is finished, in which case the
            // change signal has been and gone.
            // `sock.item` is only assigned once this component is finished, so the greeting waits
            // a turn rather than asking a socket the singleton cannot see yet.
            Component.onCompleted: if (connected) Qt.callLater(daemon._hello)
            onConnectionStateChanged: {
                if (connected) {
                    // Deferred like the one above, and for the same reason: during construction
                    // this fires before `sock.item` exists, the greeting found no socket and
                    // logged "Hello failed" on every start. `callLater` also folds the two into
                    // one greeting.
                    Qt.callLater(daemon._hello)
                } else {
                    daemon.ready = false
                    daemon._listings = ({})
                    daemon._failPending("Disconnected", "the daemon restarted")
                    daemon.retry.start()
                }
            }
        }
    }
}
