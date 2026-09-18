pragma Singleton
import QtQuick

// The far end of the fake Quickshell socket (plan 28). Everything the shell singletons send —
// Daemon.request, and so Jobs.submit, Settings writes, Favorites — lands in `sent`, and a test
// answers it with reply()/replyTo() or pushes an event with emitEvent().
//
// It is a singleton because the Daemon singleton builds its own Socket: there is no instance for
// a test to hold, so both ends meet here instead.
QtObject {
    id: wire

    property var sent: []            // every request object, in order
    property var sockets: []         // the stub sockets that registered

    function reset() { sent = [] }

    function register(s) { const a = sockets.slice(); a.push(s); sockets = a }

    function record(line) {
        try { sent = sent.concat([JSON.parse(line)]) } catch (e) { console.warn("Wire: bad json", line) }
    }

    /// Every request of a type, oldest first.
    function requests(type) { return sent.filter(r => r.type === type) }
    /// The most recent request of a type, or null.
    function last(type) { const r = requests(type); return r.length ? r[r.length - 1] : null }
    function count(type) { return requests(type).length }

    /// Hand a message to the shell exactly as kikid would.
    function deliver(msg) {
        const line = JSON.stringify(msg)
        for (let i = 0; i < sockets.length; i++) if (sockets[i].parser) sockets[i].parser.read(line)
    }
    function reply(id, ok) { deliver({ id: id, ok: ok || {} }) }
    function fail(id, code, message) { deliver({ id: id, err: { code: code || "Io", message: message || "" } }) }
    /// Answer the most recent request of a type; returns the request that was answered.
    function replyTo(type, ok) { const r = last(type); if (r) reply(r.id, ok); return r }
    function emitEvent(e) { deliver(e) }

    /// Bring the connection up, which is what makes the Daemon singleton send its Hello.
    function connectAll() {
        for (let i = 0; i < sockets.length; i++) { sockets[i].connected = true; sockets[i].connectionStateChanged() }
    }
}
