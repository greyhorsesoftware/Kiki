import QtQuick

// A daemon for the interaction tests (plan 28): it holds folders in memory, answers the listing
// protocol from them, and records every request so a test can assert what the UI asked for.
//
// Give a `WindowCache` one with `pane.listing.daemon = fake` — the cache resolves its daemon
// lazily for exactly this reason, so the real singleton and its socket never come up.
QtObject {
    id: fake

    /// uri -> array of rows. Build rows with file() and dir().
    property var tree: ({})
    /// Every request, in order: { type, fields }.
    property var sent: []

    property int _nextLid: 1
    property var _open: ({})         // lid -> { uri, filter, hidden, role, order }
    property var _bound: ({})        // lid -> the WindowCache

    // ---------------------------------------------------------------- fixtures

    function file(name, opts) {
        const o = opts || {}
        return {
            name: name, kind: o.kind || "text", isDir: false, isLink: o.isLink === true,
            meta: { size: o.size === undefined ? 1024 : o.size, mtime: o.mtime || 1700000000000, atime: o.atime || 1700000000000, mode: o.mode || 33188 },
            thumb: o.thumb || null, git: o.git || null, opened: o.opened || 0
        }
    }
    function dir(name, opts) {
        const o = opts || {}
        return {
            name: name, kind: "folder", isDir: true, isLink: false,
            meta: { size: 0, mtime: o.mtime || 1700000000000, atime: o.atime || 1700000000000, mode: o.mode || 16877 },
            thumb: null, git: o.git || null, opened: 0
        }
    }

    // ---------------------------------------------------------------- assertions

    function reset() { sent = [] }
    function requests(type) { return sent.filter(r => r.type === type) }
    function last(type) { const r = requests(type); return r.length ? r[r.length - 1] : null }
    function count(type) { return requests(type).length }

    // ---------------------------------------------------------------- the daemon interface

    function allocLid() { return _nextLid++ }
    function bind(lid, listing) { _bound[lid] = listing }
    function unbind(lid) { delete _bound[lid] }

    /// Push an event at the listing it names, as kikid's watcher would.
    function emitEvent(msg) { const l = _bound[msg.lid]; if (l) l.handleEvent(msg) }

    /// The rows a listing currently shows: hidden files, filter and sort applied in that order.
    function rowsOf(lid) {
        const st = _open[lid]
        if (!st) return []
        let rows = (tree[st.uri] || []).slice()
        if (!st.hidden) rows = rows.filter(r => r.name.indexOf(".") !== 0)
        if (st.filter) { const f = st.filter.toLowerCase(); rows = rows.filter(r => r.name.toLowerCase().indexOf(f) >= 0) }
        const role = st.role, sign = st.order === "desc" ? -1 : 1
        rows.sort((a, b) => {
            if (a.isDir !== b.isDir) return a.isDir ? -1 : 1      // folders first, as the daemon sorts
            let d = 0
            switch (role) {
            case "size": d = (a.meta.size || 0) - (b.meta.size || 0); break
            case "mtime": d = (a.meta.mtime || 0) - (b.meta.mtime || 0); break
            case "atime": d = (a.meta.atime || 0) - (b.meta.atime || 0); break
            case "kind": d = a.kind < b.kind ? -1 : (a.kind > b.kind ? 1 : 0); break
            default: d = a.name.localeCompare(b.name)
            }
            if (d === 0) d = a.name.localeCompare(b.name)
            return d * sign
        })
        return rows
    }

    function request(type, fields, cb) {
        const f = fields || {}
        sent = sent.concat([{ type: type, fields: f }])
        switch (type) {
        case "Open":
            _open[f.lid] = { uri: f.uri, filter: "", hidden: false, role: "name", order: "asc" }
            if (cb) cb({ cached: false }, undefined)
            break
        case "Window": {
            const rows = rowsOf(f.lid)
            if (cb) cb({ first: f.first, rows: rows.slice(f.first, f.first + f.count), n: rows.length, done: true }, undefined)
            break
        }
        case "Sort":
            if (_open[f.lid]) { _open[f.lid].role = f.role; _open[f.lid].order = f.order }
            if (cb) cb({}, undefined)
            _resetListing(f.lid)
            break
        case "Filter":
            if (_open[f.lid]) _open[f.lid].filter = f.text || ""
            if (cb) cb({}, undefined)
            _resetListing(f.lid)
            break
        case "ShowHidden":
            if (_open[f.lid]) _open[f.lid].hidden = f.show === true
            if (cb) cb({}, undefined)
            _resetListing(f.lid)
            break
        case "SeekName": {
            const rows = rowsOf(f.lid)
            let at = -1
            for (let i = 0; i < rows.length; i++) if (rows[i].name === f.name) { at = i; break }
            if (cb) cb({ index: at }, undefined)
            break
        }
        case "Refresh":
            if (cb) cb({}, undefined)
            _resetListing(f.lid)
            break
        case "Close":
            delete _open[f.lid]
            if (cb) cb({}, undefined)
            break
        default:
            // Everything else (Submit, Settings, Favorites …) is recorded and answered emptily;
            // a test that cares asserts on `sent`.
            if (cb) cb({}, undefined)
        }
    }

    /// What the daemon sends after a sort, filter or refresh: the window is invalid, refetch.
    function _resetListing(lid) {
        emitEvent({ event: "Reset", lid: lid, n: rowsOf(lid).length })
    }
}
