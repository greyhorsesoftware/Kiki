pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "." as Kiki
import "version.js" as Version

// The one connection to kikid. Newline-delimited JSON over the Unix socket:
// requests carry an id and get one reply; events carry "event" and a lid.
// The shell never parses more than one window of rows per message.
Singleton {
    id: daemon

    /// Named for the version — `kiki-0.2.0.sock` — so this window and a daemon of its own build
    /// find each other and no other (`version.js`); `KIKI_SOCKET` overrides it, as for the daemon.
    property string socketPath: Quickshell.env("KIKI_SOCKET") || (Quickshell.env("XDG_RUNTIME_DIR") + "/kiki-" + Version.version + ".sock")
    /// A daemon of another version answered: the sentence to show, "" when the pair matches.
    property string mismatch: ""
    /// What Hello's reply says about the pairing: "" for a daemon of this version (or one too
    /// old to say — a script's fake), else the sentence, in the window's language.
    function pairing(ok) {
        if (!ok || ok.kikid === undefined || ok.kikid === Version.version) return ""
        return Kiki.T.tr("daemon.versionMismatch", { daemon: ok.kikid, window: Version.version })
    }
    property bool ready: false
    property int protocolVersion: 0
    property string daemonVersion: ""

    // ---------------------------------------------------------------- starting the daemon
    /// The daemon is this window's engine, not a service: when nothing answers the socket, the
    /// window starts one and it leaves a moment after the last window has gone
    /// (docs/0.3.0/01-daemon-on-demand.md). `KIKI_DAEMON` names the binary — `make run` points it
    /// at the checkout's — and otherwise it is the `kikid` beside the shell, found on PATH.
    readonly property string daemonBinary: Quickshell.env("KIKI_DAEMON") || "kikid"
    /// Started once per silence, not once per retry: the daemon takes a moment to bind, and a
    /// window that started one on every tick would start a handful before the first answered.
    /// (Harmless — they stand down on a socket already served — but noisy in a process list.)
    property bool starting: false
    /// Set when a daemon we started could not be run at all: a window with no engine says so
    /// rather than retrying in silence for ever.
    property string startError: ""
    function startDaemon() {
        if (daemon.starting || daemon.ready) return
        daemon.starting = true
        starter.running = true
        // And go looking for it: a socket that has NEVER connected gets no state change to
        // notice, so without this the window started a daemon and then sat there for ever
        // beside it, connected to nothing (2026-09-26).
        daemon.tryAgain()
    }
    /// Look for the daemon from now on, quickly at first: one takes about 10 ms to bind, so a
    /// cold start that waited out the slow cadence spent most of a second on nothing.
    function tryAgain() {
        if (daemon.ready) { daemon.retry.stop(); return }
        daemon.attempts = 0
        daemon.retry.start()
    }
    property Process starter: Process {
        running: false
        command: [daemon.daemonBinary]
        // Not `SIGTERM` on exit: the daemon outlives this window on purpose when another window
        // is using it, and it knows when to leave on its own.
        onExited: (code, status) => {
            daemon.starting = false
            // Exit 0 without the window ever connecting is the "already served" stand-down, which
            // is the ordinary race between two windows; anything else is a daemon that failed.
            if (code !== 0 && !daemon.ready) {
                daemon.startError = Kiki.T.tr("daemon.startFailed", { binary: daemon.daemonBinary, code: code })
                console.warn("kikid:", daemon.startError)
            }
        }
    }

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
        // A mismatched daemon answers nothing but its refusal: every request is the sentence.
        if (daemon.mismatch !== "") { if (cb) cb(undefined, { code: "Version", message: daemon.mismatch }); return id }
        if (cb) _pending[id] = cb
        socket.write(JSON.stringify(msg) + "\n")
        socket.flush()
        return id
    }

    /// Opening something elsewhere: the one verb for it, `Launch` (0.3.0, `handlers/open.rs`;
    /// `Open` is a listing's). `how` says
    /// where — "default", "app:<desktop id>", "tool:<id or role>", "terminal" or "ai" — and the
    /// menus build that string rather than each having a verb of their own. `opts.line` is for
    /// an editor, `opts.dir` where a terminal or the AI starts (else the first uri). The daemon
    /// refuses a server's file with 1330, already a sentence in the window's language. (`how`,
    /// because `with` is a word JavaScript keeps for itself.)
    function open(uris, how, opts, cb) {
        const o = opts || {}
        const fields = { uris: uris || [], "with": how }
        if (o.line !== undefined) fields.line = o.line
        if (o.dir !== undefined) fields.dir = o.dir
        return request("Launch", fields, cb)
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

    /// kikid is this window's to start (above) and, should it die, to start again; the window
    /// should follow it back up rather than sit there looking fine and doing nothing. A dropped `Socket` will not take a
    /// second connection — setting `connected` again does nothing — so the retry builds a new one.
    /// How many times the socket has been tried since the last answer. The first second is
    /// tried hard — a daemon binds in about 10 ms — and after that the cadence is the patient
    /// one, for a daemon that is failing to start or a machine under load.
    property int attempts: 0
    readonly property int fastTries: 25
    property Timer retry: Timer {
        interval: daemon.attempts < daemon.fastTries ? 40 : 700
        repeat: true; running: false
        onTriggered: {
            if (daemon.ready) { stop(); return }
            daemon.attempts++
            // Connected but never greeted: the Hello went out into a socket that was closing.
            if (daemon.connected) { daemon._hello(); return }
            // Nothing is answering: make sure one is coming, then try the socket again.
            daemon.startDaemon()
            sock.active = false
            sock.active = true
        }
    }

    /// The handshake. Also the point where a second session is recognised as a reconnect.
    function _hello() {
        daemon.request("Hello", { version: 1, client: "kiki" }, (ok, err) => {
            if (!ok) { console.warn("kikid Hello failed", err && err.message); return }
            daemon.mismatch = daemon.pairing(ok)
            if (daemon.mismatch !== "") { console.warn("kikid", ok.kikid, "is not this window's", Version.version); return }
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
            // Nothing answered the very first connection: no daemon is running, so start one now
            // rather than after the first retry tick — this is the cold start every launch pays.
            Component.onCompleted: connected ? Qt.callLater(daemon._hello) : Qt.callLater(daemon.tryAgain)
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
                    // Fast again: a daemon that died is started here and is back in about 10 ms.
                    daemon.tryAgain()
                }
            }
        }
    }
}
