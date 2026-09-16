pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

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

    function allocLid() { return _nextLid++ }

    // Send a request; cb(result, error) is called once with the reply.
    function request(type, fields, cb) {
        const id = _nextId++
        const msg = Object.assign({ id: id, type: type }, fields || {})
        if (cb) _pending[id] = cb
        socket.write(JSON.stringify(msg) + "\n")
        socket.flush()
        return id
    }

    function bind(lid, listing) { _listings[lid] = listing }
    function unbind(lid) { delete _listings[lid] }

    function _dispatch(line) {
        let msg
        try { msg = JSON.parse(line) } catch (e) { console.warn("kikid: bad json", line); return }
        if (msg.id !== undefined && (msg.ok !== undefined || msg.err !== undefined)) {
            const cb = _pending[msg.id]
            if (cb) { delete _pending[msg.id]; cb(msg.ok, msg.err) }
            else if (msg.err) console.warn("kikid error", msg.err.code, msg.err.message)
            return
        }
        if (msg.event !== undefined) {
            if (msg.lid !== undefined && _listings[msg.lid]) _listings[msg.lid].handleEvent(msg)
            daemon.event(msg)
        }
    }

    property Socket socket: Socket {
        path: daemon.socketPath
        connected: true
        parser: SplitParser {
            splitMarker: "\n"
            onRead: line => daemon._dispatch(line)
        }
        onConnectionStateChanged: {
            if (connected) {
                daemon.request("Hello", { version: 1, client: "kiki" }, (ok, err) => {
                    if (ok) { daemon.protocolVersion = ok.version; daemon.daemonVersion = ok.daemon; daemon.ready = true }
                    else console.warn("kikid Hello failed", err && err.message)
                })
            } else {
                daemon.ready = false
            }
        }
    }
}
