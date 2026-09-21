import QtQuick
import QtTest
import "../../qml/kiki" as Kiki
import "../../qml/kiki/ui" as UI
import KikiTest

// The mirror workspace's own rules (plan 08), the ones that hold whatever the daemon answers:
// what the two footers say, what it refuses to scan, that nothing runs without a plan, and that
// closing it puts it back at Configure. The rest — a scan, a plan, a run — is the e2e flows'.
TestCase {
    id: tc
    name: "MirrorWorkspace"
    when: windowShown
    visible: true
    width: 900; height: 600

    UI.MirrorWorkspace { id: ws; anchors.fill: parent; home: "/home/t" }

    function init() {
        Wire.reset()
        ws.forget()
        ws.localUri = "file:///home/t/site"
        ws.remoteUri = "sftp://lab/srv/www"
        ws.upload = true
    }

    // The two copy counts are a sum, not a sentence: "3" and "1" read as "31 copy" for as long as
    // the string in front of them turned the addition into a join.
    function test_the_plan_footer_adds_the_copies_up() {
        ws.counts = { new: 3, changed: 1, deletes: 2, copyBytes: 2048, filtered: 6 }
        compare(ws.planSummary, "4 copy · 2 delete · 2.0 KB to transfer · 6 filtered out")
        ws.counts = ({})
        compare(ws.planSummary, "0 copy · 0 delete · 0 B to transfer · 0 filtered out")
    }

    function test_the_done_footer_says_what_came_of_the_run_data() {
        return [
            { tag: "running", run: { state: "running", bytes: 10 }, text: "a mirror run is not undoable; re-run to converge" },
            { tag: "queued", run: { state: "queued", bytes: 0 }, text: "a mirror run is not undoable; re-run to converge" },
            { tag: "done", run: { state: "done", bytes: 2048, result: { copies: 12, deletes: 1 } }, text: "Mirror complete · 12 copied · 1 deleted · 2.0 KB" },
            { tag: "done with skips", run: { state: "done", bytes: 0, result: { copies: 2, deletes: 0, skipped: 3 } }, text: "Mirror complete · 2 copied · 0 deleted · 0 B · 3 skipped" },
            { tag: "cancelled", run: { state: "cancelled", bytes: 5 }, text: "Mirror cancelled" },
            { tag: "failed", run: { state: "failed", bytes: 0, error: "no such plan" }, text: "Mirror failed: no such plan" },
        ]
    }
    function test_the_done_footer_says_what_came_of_the_run(d) {
        ws.runInfo = d.run
        compare(ws.doneText, d.text)
    }
    function test_the_done_footer_says_what_came_of_the_run_cleanup() { ws.runInfo = null }

    // A folder mirrored into something inside it copies its own tree into itself — and with
    // deletes on takes what it has just made for extras. Refused before the scan.
    function test_a_folder_is_never_mirrored_into_itself_data() {
        return [
            { tag: "into a child", local: "file:///home/t/site", remote: "file:///home/t/site/dist", up: true, says: "inside the source" },
            { tag: "out of a child", local: "file:///home/t/site", remote: "file:///home/t/site/dist", up: false, says: "inside the destination" },
            { tag: "onto itself", local: "file:///home/t/site", remote: "file:///home/t/site", up: true, says: "the same folder" },
            { tag: "a trailing slash is the same folder", local: "file:///home/t/site/", remote: "file:///home/t/site", up: true, says: "the same folder" },
        ]
    }
    function test_a_folder_is_never_mirrored_into_itself(d) {
        ws.localUri = d.local; ws.remoteUri = d.remote; ws.upload = d.up
        ws.preflight()
        compare(ws.screen, "configure")
        verify(ws.status.indexOf(d.says) >= 0, ws.status)
        compare(Wire.count("Submit"), 0)
    }

    function test_two_folders_side_by_side_are_scanned() {
        ws.preflight()
        compare(ws.screen, "preflight")
        const sent = Wire.last("Submit")
        verify(sent && sent.op.op === "mirrorScan", JSON.stringify(sent))
        compare(sent.op.spec.master, "file:///home/t/site")
        compare(sent.op.spec.replica, "sftp://lab/srv/www")
    }

    // Preview is mandatory: the Mirror button is only on the Review screen, and the run holds to
    // the same rule wherever it is called from.
    function test_nothing_runs_without_a_plan() {
        ws.mirror(false)
        compare(Wire.count("Submit"), 0)
        compare(ws.screen, "configure")
    }

    // The options are one function each, so a script drives the very code a click does.
    function test_the_configure_options_are_what_the_controls_set() {
        ws.setOption("detector", "sizeOnly"); compare(ws.detector, "sizeOnly")
        ws.setOption("detector", "auto"); compare(ws.detector, "auto")
        ws.setOption("deletes", "on"); compare(ws.deleteExtras, true)
        ws.setOption("deletes", "off"); compare(ws.deleteExtras, false)
        ws.setOption("filters", "off"); compare(ws.applyFilters, false)
        ws.setOption("window", "on"); ws.setOption("windowValue", "3"); ws.setOption("windowUnit", "weeks")
        compare(ws.spec().modifiedWithinMs, 3 * 604800000)
        ws.setOption("direction", "download")
        compare(ws.spec().master, "sftp://lab/srv/www")
        // Locked once the scan is away: the header's arrows do nothing off the Configure screen.
        ws.screen = "review"
        ws.setOption("direction", "upload")
        compare(ws.upload, false)
    }

    // The clock offset (plan 08). The engine compares `master.mtime − offset − replica.mtime`, so
    // the hours are what the SOURCE reads more than the destination: +3 is a destination three
    // hours behind. A sign the wrong way about re-copies a whole tree or skips one, so what the
    // spec carries and what the row says are both checked here.
    function test_the_clock_offset_is_automatic_until_it_is_set_by_hand() {
        compare(ws.offsetAuto, true)
        compare(ws.spec().clockOffsetAuto, true)
        compare(ws.spec().clockOffsetMs, 0)
        compare(ws.offsetText, "Determined automatically from files present on both sides")

        ws.setOption("offset", "3")
        compare(ws.offsetAuto, false)
        compare(ws.offsetHours, 3)
        compare(ws.spec().clockOffsetAuto, false)
        compare(ws.spec().clockOffsetMs, 3 * 3600000)
        compare(ws.offsetText, "The destination's clock is 3 hours behind the source")

        ws.setOption("offset", "-1")
        compare(ws.spec().clockOffsetMs, -3600000)
        compare(ws.offsetText, "The destination's clock is 1 hour ahead of the source")

        ws.setOption("offset", "0")
        compare(ws.offsetText, "Both sides' clocks read the same")

        // Back to automatic: the hours are kept, but nothing is sent with them.
        ws.setOption("offset", "2")
        ws.setOption("offset", "auto")
        compare(ws.offsetAuto, true)
        compare(ws.offsetHours, 2)
        compare(ws.spec().clockOffsetMs, 0, "automatic: the daemon measures it, so nothing is imposed")
    }

    // Whole hours, −24 to +24. Anything else is refused, and refusing changes nothing at all.
    function test_an_offset_outside_a_day_either_way_is_refused_data() {
        return [
            { tag: "past a day ahead", v: "25", takes: false },
            { tag: "past a day behind", v: "-25", takes: false },
            { tag: "not a number", v: "soon", takes: false },
            { tag: "nothing at all", v: "", takes: false },
            { tag: "a day ahead", v: "24", takes: true },
            { tag: "a day behind", v: "-24", takes: true },
        ]
    }
    function test_an_offset_outside_a_day_either_way_is_refused(d) {
        ws.setOption("offset", "5")
        compare(ws.setOffsetHours(d.v), d.takes)
        compare(ws.offsetHours, d.takes ? parseInt(d.v) : 5)
        // And it is refused the same way when it is what turns the check on.
        ws.setOption("offset", "auto")
        ws.setOption("offset", d.v)
        compare(ws.offsetAuto, !d.takes, "a refused number does not press the check either")
    }
    function test_an_offset_outside_a_day_either_way_is_refused_cleanup() { ws.setOption("offset", "auto"); ws.setOffsetHours(0) }

    // Dimmed while automatic, in the way the window's number box is while its check is off.
    function test_the_hours_box_is_dimmed_and_dead_while_the_offset_is_automatic() {
        const box = findChild(ws, "mirror-offset-box"), hours = findChild(ws, "mirror-offset-hours")
        verify(box && hours, "the offset row is on the Configure screen")
        ws.setOption("offset", "auto")
        compare(hours.enabled, false)
        verify(box.opacity < 1, box.opacity)
        ws.setOption("offset", "4")
        compare(hours.enabled, true)
        compare(box.opacity, 1)
        compare(hours.text, "4", "and the box shows what a script set")
    }
    function test_the_hours_box_is_dimmed_and_dead_while_the_offset_is_automatic_cleanup() { ws.setOption("offset", "auto"); ws.setOffsetHours(0) }

    // Closing puts it back at Configure — it used to come back on the table of the last run —
    // unless a run is still going, which the window goes on following to the end.
    function test_closing_forgets_a_run_that_is_over() {
        ws.screen = "running"; ws.counts = { new: 4 }; ws.scanJob = 7; ws.runJob = 8
        ws.runInfo = { state: "done", bytes: 0, result: {} }
        ws.leave()
        compare(ws.screen, "configure")
        compare(ws.runInfo, null)
        compare(ws.scanJob, 0)
    }
    function test_closing_keeps_watching_a_run_that_is_not() {
        ws.screen = "running"; ws.runJob = 9
        ws.runInfo = { state: "running", bytes: 1 }
        ws.leave()
        compare(ws.screen, "running")
        compare(ws.runJob, 9)
    }

    // ---- cancelling the compare (plan 08's Preflight screen).

    // The way out of a compare is the word the Running screen uses for stopping a run, in the
    // place a button that does something is looked for. It read "Back" until somebody waiting on
    // a compare of a server went looking for a way to stop it and did not find one.
    function test_the_preflight_screen_offers_cancel_not_back() {
        ws.preflight()
        compare(ws.screen, "preflight")
        const b = findChild(ws, "mirror-stop")
        verify(b && b.visible, "the button that stops the compare is on the footer")
        compare(b.text, "Cancel")
        compare(b.primary, true, "and it is where the button that does something is")
        b.clicked()
        compare(ws.screen, "configure")
    }

    // Pressing it cancels the job and leaves the form as it was found: no plan, no id to catch a
    // late event, and nothing red left over.
    function test_cancel_on_preflight_stops_the_compare_and_clears_up() {
        ws.preflight()
        Wire.replyTo("Submit", { job: 11 })
        compare(ws.scanJob, 11)
        ws.stop()
        compare(Wire.last("Cancel").job, 11)
        compare(ws.screen, "configure")
        compare(ws.scanJob, 0, "the id is forgotten, so a late event for it lands on nobody")
        compare(ws.info().status, "Compare cancelled")
        compare(ws.info().statusError, false, "a compare that was stopped is not an error")
    }

    // The reply to Submit and the job's events travel separately: a Cancel pressed before the id
    // is known must still stop the job once it arrives. It used to be dropped, and the compare
    // went on running where nobody could see it.
    function test_a_cancel_before_the_job_has_an_id_still_cancels_it() {
        ws.preflight()
        compare(Wire.count("Submit"), 1)
        ws.stop()
        compare(ws.screen, "configure")
        compare(Wire.count("Cancel"), 0, "there is nothing to cancel yet")
        Wire.replyTo("Submit", { job: 12 })
        const sent = Wire.last("Cancel")
        verify(sent, "the id arrived and the cancel went after it")
        compare(sent.job, 12)
        compare(ws.scanJob, 0, "and the workspace never took the job on")
        compare(ws.screen, "configure")
    }

    // Escape is the same Cancel, for a hand already on the keyboard. The other screens keep
    // whatever Escape does elsewhere in the window.
    function test_escape_cancels_the_compare_and_leaves_the_other_screens_alone() {
        ws.preflight()
        Wire.replyTo("Submit", { job: 13 })
        compare(ws.escapeKey(), true)
        compare(Wire.last("Cancel").job, 13)
        compare(ws.screen, "configure")
        compare(ws.escapeKey(), false, "nothing to stop on Configure")
        ws.screen = "review"
        compare(ws.escapeKey(), false)
        compare(ws.screen, "review", "Review is left where it was")
        ws.screen = "running"
        compare(ws.escapeKey(), false, "a run is stopped by the button, not by a keypress")
    }

    // Something IS happening, and how far it has got: the daemon counts entries as it walks, and
    // the screen counts them up. There is no bar — nobody knows how big a tree is until it has
    // been walked.
    function test_the_preflight_screen_says_how_far_the_compare_has_got() {
        ws.preflight()
        Wire.replyTo("Submit", { job: 14 })
        compare(ws.info().status, "Comparing ~/site…")
        Wire.emitEvent({ event: "JobEvent", job: { id: 14, op: "mirrorScan", state: "running", done: 12400, total: 0 } })
        compare(ws.info().status, "Comparing ~/site…  12,400 items so far")
        compare(ws.info().scanning, 12400)
        compare(ws.info().statusError, false)
        // Cancelled, the count goes with it.
        ws.stop()
        compare(ws.info().status, "Compare cancelled")
        compare(ws.info().scanning, 0)
    }

    // The daemon's own word for it, for a compare cancelled from anywhere else — the activity
    // view, another window — lands in the same place.
    function test_a_compare_the_daemon_says_was_cancelled_comes_back_to_configure() {
        ws.preflight()
        Wire.replyTo("Submit", { job: 15 })
        Wire.emitEvent({ event: "JobEvent", job: { id: 15, op: "mirrorScan", state: "cancelled", done: 900, total: 0 } })
        compare(ws.screen, "configure")
        compare(ws.info().status, "Compare cancelled")
        compare(ws.info().statusError, false)
        compare(ws.scanJob, 0)
    }

    // "I hit Back, then went back into the screen and it was still showing Comparing… text."
    // Every way back to the form leaves it as a form: no word about a compare that is over.
    function test_no_way_back_to_configure_leaves_comparing_on_the_screen_data() {
        return [{ tag: "Back from Preflight", how: "back" }, { tag: "Cancel from Preflight", how: "stop" }, { tag: "Escape from Preflight", how: "escape" },
                { tag: "leaving and opening again", how: "leave" }]
    }
    function test_no_way_back_to_configure_leaves_comparing_on_the_screen(d) {
        ws.preflight()
        Wire.replyTo("Submit", { job: 16 })
        Wire.emitEvent({ event: "JobEvent", job: { id: 16, op: "mirrorScan", state: "running", done: 800, total: 0 } })
        verify(ws.info().status.indexOf("Comparing") === 0, ws.info().status)
        if (d.how === "leave") ws.leave(); else if (d.how === "escape") ws.escapeKey(); else if (d.how === "stop") ws.stop(); else ws.back()
        compare(ws.screen, "configure")
        verify(ws.info().status.indexOf("Comparing") < 0, "the form says nothing about a compare that is over: " + ws.info().status)
        compare(ws.info().statusError, false)
        // And the status line on the form is only red when it is a refusal.
        const t = findChild(ws, "mirror-status")
        verify(t, "the status line is on the Configure screen")
        if (t.visible) verify(String(t.color) !== String(Kiki.Theme.danger), "a compare that was stopped is not drawn as an error")
    }

    // A `done` for a compare that was backed out of must not pull the workspace through to the
    // Review screen behind the user's back, and pressing Preflight again is a fresh compare whose
    // own result is the only one that counts.
    function test_a_late_answer_for_a_cancelled_compare_is_ignored() {
        ws.preflight()
        Wire.replyTo("Submit", { job: 17 })
        ws.back()
        compare(ws.screen, "configure")
        Wire.emitEvent({ event: "JobEvent", job: { id: 17, op: "mirrorScan", state: "done", done: 5, total: 5 } })
        compare(ws.screen, "configure", "the compare that was cancelled does not open its plan")
        compare(Wire.count("MirrorPlan"), 0)

        ws.preflight()
        Wire.replyTo("Submit", { job: 18 })
        compare(ws.scanJob, 18)
        Wire.emitEvent({ event: "JobEvent", job: { id: 17, op: "mirrorScan", state: "done", done: 5, total: 5 } })
        compare(Wire.count("MirrorPlan"), 0, "and neither does it once a new one is going")
        Wire.emitEvent({ event: "JobEvent", job: { id: 18, op: "mirrorScan", state: "done", done: 5, total: 5 } })
        compare(Wire.count("MirrorPlan"), 1, "only this compare's own result opens the plan")
        compare(Wire.last("MirrorPlan").job, 18)
    }

    // Shutting the workspace on a compare that is still going stops it too — Ctrl+M closes the
    // workspace from any screen, and a compare left walking is one nobody can see or stop.
    function test_closing_the_workspace_stops_a_compare_that_is_still_going() {
        ws.preflight()
        Wire.replyTo("Submit", { job: 19 })
        ws.leave()
        compare(Wire.last("Cancel").job, 19)
        compare(ws.screen, "configure")
        compare(ws.info().status, "")
    }

    // A refusal IS red, and says so to anything reading the workspace.
    function test_a_refusal_is_still_drawn_as_one() {
        ws.remoteUri = "file:///home/t/site/dist"
        ws.preflight()
        compare(ws.screen, "configure")
        compare(ws.info().statusError, true)
        const t = findChild(ws, "mirror-status")
        verify(t && t.visible)
        compare(String(t.color), String(Kiki.Theme.danger))
    }

    // ---- the filter rules (plan 08). The dialog is tst_MirrorRulesDialog's; what is here is
    // what the workspace does with it.

    // Edit rules… sits beside the check and works whether or not it is on: what the rules say is
    // worth reading before deciding to apply them.
    function test_edit_rules_is_beside_the_check_and_never_dimmed() {
        const b = findChild(ws, "mirror-edit-rules")
        verify(b, "the button is on the Configure screen")
        compare(b.text, "Edit rules…")
        compare(b.enabled, true)
        ws.setOption("filters", "off")
        compare(b.enabled, true, "the rules are edited with the check off too")
        ws.setOption("filters", "on")
    }
    function test_edit_rules_is_beside_the_check_and_never_dimmed_cleanup() { ws.setOption("filters", "on") }

    /// The built-in set the daemon sends with every answer: eight names, not `startsWith "."` —
    /// `.htaccess` and `.well-known/` are mirrored now unless a rule names them.
    readonly property var builtIn: [
        { kind: "matches", value: ".git" }, { kind: "matches", value: ".DS_Store" }, { kind: "matches", value: ".env" },
        { kind: "matches", value: ".idea" }, { kind: "matches", value: ".vscode" }, { kind: "matches", value: "Thumbs.db" },
        { kind: "matches", value: "node_modules" }, { kind: "matches", value: "__pycache__" }]

    function test_the_workspace_reads_the_rules_and_reports_them() {
        ws.loadRules()
        const asked = Wire.replyTo("MirrorFilters", { rules: [{ kind: "startsWith", value: "." }, { kind: "matches", value: "target" }], defaults: false, defaultRules: tc.builtIn })
        verify(asked, "it asks the daemon")
        const st = ws.info()
        compare(st.rules, [{ kind: "startsWith", value: "." }, { kind: "matches", value: "target" }])
        compare(st.rulesDefault, false)
    }

    // The built-in set is the daemon's and nobody keeps a second copy: Edit rules… opens on a
    // reply, and Restore defaults previews the `defaultRules` that reply carried.
    function test_restore_defaults_previews_the_set_the_daemon_sent() {
        ws.editRules()
        Wire.replyTo("MirrorFilters", { rules: [{ kind: "endsWith", value: ".bak" }], defaults: false, defaultRules: tc.builtIn })
        compare(ws.info().rulesDialog.rules.map(r => r.value), [".bak"])
        findChild(ws, "mirror-rules-dialog").restoreDefaults()
        compare(ws.info().rulesDialog.rules.map(r => r.value), [".git", ".DS_Store", ".env", ".idea", ".vscode", "Thumbs.db", "node_modules", "__pycache__"])
        ws.cancelRules()
    }

    // A plan on the Review screen was made with the rules as they were at scan. Change them and
    // it is stale, so the workspace goes back to the form rather than offering to run it.
    function test_saving_rules_over_a_plan_goes_back_to_configure() {
        ws.screen = "review"; ws.scanJob = 3
        ws.setRules("endsWith .tmp\nmatches node_modules")
        const sent = Wire.last("SetMirrorFilters")
        verify(sent, "the script's rules go through the dialog's own save")
        compare(sent.rules, [{ kind: "endsWith", value: ".tmp" }, { kind: "matches", value: "node_modules" }])
        compare(ws.screen, "review", "nothing moves until the daemon has taken them")
        Wire.replyTo("SetMirrorFilters", { rules: sent.rules, defaults: false, defaultRules: tc.builtIn })
        compare(ws.screen, "configure")
        compare(ws.info().rules, sent.rules, "and the workspace is showing what was saved")
        compare(ws.info().rulesDefault, false)
    }

    // Nothing at all is a real answer — transfer everything — and not the same as the defaults.
    function test_a_script_can_delete_every_rule() {
        ws.setRules("")
        const sent = Wire.last("SetMirrorFilters")
        compare(sent.rules, [])
    }

    function test_a_script_can_restore_the_defaults() {
        ws.restoreRules()
        const sent = Wire.last("SetMirrorFilters")
        compare(sent.defaults, true)
        verify(sent.rules === undefined, "restoring takes the file away rather than writing rules")
    }

    // The footer's buttons are inside the window at any width. They were one row with a fixed
    // spacer: at the owner's window "Save report…" hung over the edge, unclickable, and "Mirror"
    // — the button the Review screen exists for — was off it altogether.
    function footerButtons() {
        const out = []
        function walk(it) { for (const c of it.children) { if (c.text !== undefined && c.clicked !== undefined && c.visible) out.push(c); walk(c) } }
        walk(findChild(ws, "mirror-footer-right")); walk(findChild(ws, "mirror-footer-left"))
        return out
    }
    function test_the_footer_keeps_its_buttons_inside_the_window_data() { return [{ tag: "narrow", w: 640 }, { tag: "the owner's", w: 1132 }, { tag: "wide", w: 1900 }] }
    function test_the_footer_keeps_its_buttons_inside_the_window(data) {
        const holder = ws.parent, was = holder.width
        ws.anchors.fill = undefined; ws.width = data.w; ws.height = 600
        ws.screen = "review"
        ws.counts = { copies: 34, deletes: 0, bytes: 8074035, filtered: 1, replicaEntries: 40 }
        wait(20)
        const labels = footerButtons().map(b => b.text)
        verify(labels.indexOf("Mirror") >= 0 && labels.indexOf("Save report…") >= 0 && labels.indexOf("Back") >= 0, labels.join(","))
        for (const b of footerButtons()) {
            const p = b.mapToItem(ws, 0, 0)
            verify(p.x >= 0 && p.x + b.width <= ws.width, b.text + " runs from " + p.x + " to " + (p.x + b.width) + " in a window of " + ws.width)
        }
        // And the summary stops short of them rather than running underneath.
        const sum = findChild(ws, "mirror-plan-summary"), right = findChild(ws, "mirror-footer-right")
        verify(sum.mapToItem(ws, sum.width, 0).x <= right.mapToItem(ws, 0, 0).x, "the summary runs under the buttons")
        ws.screen = "configure"; ws.anchors.fill = holder
    }

    // A long path in the header wraps inside its column, and the header grows for it.
    function test_long_paths_in_the_header_wrap() {
        const was = { l: ws.localUri, r: ws.remoteUri }
        ws.localUri = "file:///home/t/Projects/a-client-with-a-long-name/sites/the-one-that-went-live-in-march/public_html/assets"
        ws.remoteUri = "sftp://ghs/home/djclark/greyhorsesoftware.com/public_html/a/very/deep/folder/that/keeps/on/going"
        wait(20)
        const l = findChild(ws, "mirror-local-path"), r = findChild(ws, "mirror-remote-path")
        verify(l.lineCount > 1, "the local path did not wrap: " + l.lineCount)
        verify(r.lineCount > 1, "the remote path did not wrap: " + r.lineCount)
        verify(l.mapToItem(ws, l.width, 0).x <= ws.width / 2 - 34 + 1, "the local path runs into the arrows")
        verify(r.mapToItem(ws, 0, 0).x >= ws.width / 2 + 34 - 1, "the remote path runs into the arrows")
        verify(r.mapToItem(ws, r.width, 0).x <= ws.width, "off the edge")
        ws.localUri = was.l; ws.remoteUri = was.r
    }

    // Configure in a short window: the PLAN box stays above the footer, its type shrinking to fit
    // and no smaller than 12 px; in a roomy one it is the full size.
    function test_the_plan_box_fits_a_short_window_data() { return [{ tag: "roomy", h: 760, small: false }, { tag: "the owner's", h: 590, small: true }, { tag: "tiny", h: 480, small: true }] }
    function test_the_plan_box_fits_a_short_window(data) {
        const holder = ws.parent
        ws.anchors.fill = undefined; ws.width = 1176; ws.height = data.h
        ws.screen = "configure"; ws.deleteExtras = true
        wait(30)
        const box = findChild(ws, "mirror-plan-box"), text = findChild(ws, "mirror-plan-text"), footer = findChild(ws, "mirror-footer-right")
        const bottom = box.mapToItem(ws, 0, box.height).y, footTop = footer.mapToItem(ws, 0, 0).y
        verify(bottom <= footTop, "the plan box runs into the footer: " + bottom + " > " + footTop)
        verify(text.fontInfo.pixelSize >= 12, "smaller than 12 px: " + text.fontInfo.pixelSize)
        if (data.small) verify(text.fontInfo.pixelSize < Kiki.Theme.fontSize || box.height < text.contentHeight + 40 + 1, "nothing gave way in a short window")
        else compare(text.fontInfo.pixelSize, Kiki.Theme.fontSize)
        // At the 12 px floor the box may clip — that is the floor the owner asked for; above it,
        // every word is on screen.
        if (text.fontInfo.pixelSize > 12) verify(text.contentHeight <= text.height + 1, "the plan's words are cut off at " + text.fontInfo.pixelSize + " px")
        ws.deleteExtras = false; ws.anchors.fill = holder
    }
}
