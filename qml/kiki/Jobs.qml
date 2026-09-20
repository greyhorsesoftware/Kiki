pragma Singleton
import QtQuick
import "." as Kiki

// Jobs as the daemon reports them, plus toasts and collision prompts — and, for the activity view
// (plan 32), which of them are worth showing and what to say about each.
QtObject {
    id: jobs
    property var list: []            // [Job]
    property var toast: null         // { job, text, undoable, until }
    property var prompt: null        // { job, kind, uri, existing, incoming, choices }
    signal changed()

    function submit(op, cb) { Kiki.Daemon.request("Submit", { op: op }, cb) }
    function cancel(id) { Kiki.Daemon.request("Cancel", { job: id }) }
    function undo() { Kiki.Daemon.request("Undo", {}, (ok, err) => { if (err) jobs.showToast({ text: "Nothing to undo", undoable: false }) }) }
    function redo() { Kiki.Daemon.request("Redo", {}) }
    function reply(choice, all) { if (prompt) { Kiki.Daemon.request("PromptReply", { job: prompt.job, choice: choice, applyToAll: !!all }); prompt = null } }
    function live(j) { return j.state === "running" || j.state === "queued" }
    function running() { return list.filter(live) }
    function showToast(t) { toast = t; toastTimer.restart() }
    function dismissToast() { toast = null; toastTimer.stop() }
    /// Forgetting is the daemon's to do — the list is fetched again on every reconnect — and it
    /// answers with `JobsCleared`, which is what takes them out of `list`.
    function clear() { Kiki.Daemon.request("ClearJobs", {}) }
    function dismiss(id) { Kiki.Daemon.request("DismissJob", { job: id }) }

    // ---------------------------------------------------------------- the activity view

    /// One step and gone: shown while they run and if they go wrong, not kept as history. (A
    /// list of "Deleted 1 item" is noise beside the transfers somebody is waiting on.)
    readonly property var quietWhenDone: ["delete", "chmod", "trash", "restore", "mkdir", "rename", "emptyTrash"]
    /// What the popup lists: not the machinery (an undo's inverse ops, a mirror's preflight), and
    /// not the quiet ones once they have finished well. In the order they were asked for — an
    /// entry keeps its place when it finishes.
    function shown() { return list.filter(j => !j.hidden && !(j.state === "done" && quietWhenDone.indexOf(j.op) >= 0)) }

    /// The highest failed job the popup has been opened on. A failure nobody saw is still news.
    property int seenFailure: 0
    function unseenFailures() { return shown().filter(j => j.state === "failed" && j.id > seenFailure) }
    function markSeen() { seenFailure = list.reduce((m, j) => j.state === "failed" ? Math.max(m, j.id) : m, seenFailure) }
    /// `failed` outranks `running`: something is always running somewhere.
    function orbState() { return unseenFailures().length ? "failed" : (running().filter(j => !j.hidden).length ? "running" : "idle") }
    function orbTip() {
        const f = unseenFailures().length, r = running().filter(j => !j.hidden).length
        return f ? f + " failed" + (r ? " · " + r + " running" : "") : (r ? r + " running" : "No activity")
    }

    function isTransfer(j) { return j.op === "copy" || j.op === "move" || j.op === "share" }
    function isMirror(j) { return j.op === "mirrorRun" }
    /// The bold line: the thing itself, not a sentence about it.
    function headline(j) {
        if (j.cancelling && live(j)) return "Cancelling…"
        if (!j.name) return j.title
        return j.count > 1 ? j.name + " and " + (j.count - 1) + " more" : j.name
    }
    /// No bar at all / a bar with no length yet / a fraction.
    function barMode(j) { return !live(j) ? "none" : (j.state === "queued" || j.phase === "preparing" || j.cancelling || !(j.total || j.bytesTotal) ? "busy" : "value") }
    /// A single file is measured in bytes; many, in files — a bar of bytes stalls on the big one.
    function fraction(j) {
        if (j.total <= 1 && j.bytesTotal) return Math.min(1, j.bytes / j.bytesTotal)
        return j.total ? Math.min(1, j.done / j.total) : 0
    }
    function rateText(j) { return j.rate > 0 ? Kiki.Format.transferSize(j.rate) + "/sec" : "" }
    /// The dim line under the bar while it runs.
    function statusLine(j) {
        if (j.state === "queued") return "Waiting…"
        if (j.cancelling) return ""
        if (j.phase === "preparing") return isMirror(j) ? "Preparing…" : (isTransfer(j) ? "Preparing to transfer…" : "Preparing…")
        const rate = rateText(j)
        if (isMirror(j)) return j.done + " of " + j.total + " items · " + Kiki.Format.transferSize(j.bytes) + " of " + Kiki.Format.transferSize(j.bytesTotal) + (rate ? " · " + rate : "")
        if (isTransfer(j) || j.op === "extract" || j.op === "compress") {
            if (j.total <= 1 && j.bytesTotal) return Kiki.Format.transferSize(j.bytes) + " of " + Kiki.Format.transferSize(j.bytesTotal) + (rate ? " (" + rate + ")" : "")
            return j.done + " of " + j.total + " transferred — " + Math.floor(fraction(j) * 100) + "% complete"
        }
        return "Processing " + j.done + " of " + j.total + " items"
    }
    /// The file in hand, for the row that opens under a transfer. "" when there is nothing to add
    /// to the line above — one file is its own detail.
    function detailLine(j) {
        const c = j.current
        if (!c || !(j.total > 1 || isMirror(j))) return ""
        if (!c.size) return ""
        const rate = rateText(j)
        return Kiki.Format.transferSize(c.bytes) + " of " + Kiki.Format.transferSize(c.size) + (rate ? " (" + rate + ")" : "")
    }
    function hasDetail(j) { return live(j) && j.phase !== "preparing" && !j.cancelling && !!j.current && (j.total > 1 || isMirror(j)) }
    function items(n) { return n + (n === 1 ? " item" : " items") }
    /// What replaces the bar once it is over.
    function completion(j) {
        if (j.state === "failed") return Kiki.Format.cleanError(j.error)
        if (j.state === "cancelled") return isMirror(j) ? "Mirror cancelled" : "Cancelled"
        if (isMirror(j)) {
            const r = j.result || {}
            return (r.copies || 0) + " copied, " + (r.deletes || 0) + " deleted" + (r.skipped ? ", " + r.skipped + " skipped" : "")
        }
        const n = items(Math.max(j.done, 1))
        switch (j.op) {
        case "copy": case "move":
            if (j.direction === "download") return "Downloaded " + n
            if (j.direction === "upload") return "Uploaded " + n
            return (j.op === "move" ? "Moved " : "Copied ") + n
        case "delete": return "Deleted " + n
        case "trash": return "Moved " + n + " to the trash"
        case "chmod": return "Changed permissions on " + n
        case "extract": return "Extracted"
        case "compress": return "Compressed " + n
        case "share": return "Shared " + n
        default: return "Done"
        }
    }

    function _upsert(job) {
        const l = list.slice()
        const i = l.findIndex(j => j.id === job.id)
        if (i >= 0) l[i] = job; else l.push(job)
        // Every job still going, however old, and the fifty most recent that are not: the same
        // rule as the daemon's. ("The last fifty of everything" lost a long transfer.)
        const over = l.filter(j => !live(j))
        const cut = over.length > 50 ? over[over.length - 50].id : 0
        list = l.filter(j => live(j) || j.id >= cut)
        changed()
    }

    property Timer toastTimer: Timer { interval: Kiki.Settings.timers.toastMs; onTriggered: jobs.toast = null }
    property Connections c: Connections {
        target: Kiki.Daemon
        function onReadyChanged() { if (Kiki.Daemon.ready) { Kiki.Daemon.request("JobEvents", {}); Kiki.Daemon.request("Jobs", {}, ok => { if (ok) { jobs.list = ok.jobs; jobs.changed() } }) } }
        function onEvent(msg) {
            if (msg.event === "JobEvent") jobs._upsert(msg.job)
            else if (msg.event === "JobsCleared") { const gone = msg.jobs || []; jobs.list = jobs.list.filter(j => gone.indexOf(j.id) < 0); jobs.changed() }
            else if (msg.event === "Toast") jobs.showToast({ job: msg.job, text: msg.text, undoable: msg.undoable })
            else if (msg.event === "Prompt") jobs.prompt = msg
        }
    }
}
