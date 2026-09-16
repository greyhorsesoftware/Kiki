pragma Singleton
import QtQuick
import "." as Kiki

// Jobs as the daemon reports them, plus toasts and collision prompts.
Singleton {
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
    function running() { return list.filter(j => j.state === "running" || j.state === "queued") }
    function showToast(t) { toast = t; toastTimer.restart() }
    function dismissToast() { toast = null; toastTimer.stop() }

    function _upsert(job) {
        const l = list.slice()
        const i = l.findIndex(j => j.id === job.id)
        if (i >= 0) l[i] = job; else l.push(job)
        list = l.slice(-50)
        changed()
    }

    property Timer toastTimer: Timer { interval: Kiki.Settings.timers.toastMs; onTriggered: jobs.toast = null }
    property Connections c: Connections {
        target: Kiki.Daemon
        function onReadyChanged() { if (Kiki.Daemon.ready) { Kiki.Daemon.request("JobEvents", {}); Kiki.Daemon.request("Jobs", {}, ok => { if (ok) { jobs.list = ok.jobs; jobs.changed() } }) } }
        function onEvent(msg) {
            if (msg.event === "JobEvent") jobs._upsert(msg.job)
            else if (msg.event === "Toast") jobs.showToast({ job: msg.job, text: msg.text, undoable: msg.undoable })
            else if (msg.event === "Prompt") jobs.prompt = msg
        }
    }
}
