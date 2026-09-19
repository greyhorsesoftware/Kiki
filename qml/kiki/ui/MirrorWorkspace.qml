import QtQuick
import Quickshell
import ".." as Kiki

// Configure → Preflight → Review → Running. Replaces the split panes while open.
Rectangle {
    id: ws
    property string localUri: ""
    property string remoteUri: ""
    property string home: ""
    signal closed()
    signal relist()

    color: Kiki.Theme.bg
    property string screen: "configure"     // configure | preflight | review | running
    property bool upload: true
    property string detector: "auto"
    property bool deleteExtras: false
    property bool applyFilters: true
    property bool windowOn: false
    property int windowValue: 7
    property string windowUnit: "days"
    property int scanJob: 0
    property int runJob: 0
    property var counts: ({})
    property string reviewTab: "all"
    property string status: ""
    property int concurrency: 3
    property var runInfo: null

    property Kiki.WindowCache plan: Kiki.WindowCache { padAhead: 100; padBehind: 50 }

    function spec() {
        const master = upload ? localUri : remoteUri, replica = upload ? remoteUri : localUri
        const ms = { hours: 3600000, days: 86400000, weeks: 604800000 }[windowUnit]
        return { master: master, replica: replica, direction: upload ? "upload" : "download", deleteExtras: deleteExtras, blastRadius: 0.5, confirmedLargeDelete: false, clockOffsetMs: 0, clockOffsetAuto: true, detector: detector, modifiedWithinMs: windowOn ? windowValue * ms : null, applyFilters: applyFilters }
    }
    function preflight() {
        screen = "preflight"; status = "Comparing " + Kiki.Format.display(localUri, home) + "…"
        Kiki.Jobs.submit({ op: "mirrorScan", spec: spec() }, (ok, err) => { if (err) { status = err.message; screen = "configure"; return } scanJob = ok.job })
    }
    function openPlan() {
        plan.close()
        plan.lid = Kiki.Daemon.allocLid(); Kiki.Daemon.bind(plan.lid, plan)
        Kiki.Daemon.request("MirrorPlan", { job: scanJob, lid: plan.lid }, (ok, err) => {
            if (err) { status = err.message; screen = "configure"; return }
            counts = ok.counts; plan._rows = ({}); plan.count = ok.n; plan.done = true
            plan._request(0, 100)
            screen = "review"
        })
    }
    function setTab(t) { reviewTab = t; Kiki.Daemon.request("MirrorFilter", { lid: plan.lid, reason: t }) }
    function toggleRow(i, checked) { Kiki.Daemon.request("MirrorCheck", { lid: plan.lid, first: i, count: 1, checked: checked }, ok => { if (ok) { counts = ok.counts; plan._request(Math.max(0, i - 1), 3) } }) }
    function mirror(confirmed) {
        const deletes = counts.deletes || 0, replica = counts.replicaEntries || 0
        if (deletes > 0 && replica > 0 && deletes / replica > 0.5 && !confirmed) { confirmBox.visible = true; return }
        const s = spec(); s.confirmedLargeDelete = !!confirmed
        Kiki.Jobs.submit({ op: "mirrorRun", plan: scanJob, spec: s, workers: concurrency }, (ok, err) => { if (err) { status = err.message; return } runJob = ok.job; screen = "running"; setTab("all") })
    }
    function cancelRun() { if (runJob) Kiki.Jobs.cancel(runJob) }
    function leave() { plan.close(); ws.closed() }

    Connections {
        target: Kiki.Daemon
        function onEvent(msg) {
            if (msg.event !== "JobEvent") return
            const j = msg.job
            if (j.id === ws.scanJob) {
                if (j.state === "done") ws.openPlan()
                else if (j.state === "failed") { ws.status = j.error || "scan failed"; ws.screen = "configure" }
                else if (j.state === "cancelled") ws.screen = "configure"
            } else if (j.id === ws.runJob) {
                ws.runInfo = j
                // Refetch the visible window so per-action states update.
                ws.plan._request(ws.plan.viewportFirst, Math.min(ws.plan.viewportCount + 20, 200))
                if (j.state === "done" || j.state === "failed" || j.state === "cancelled") ws.relist()
            }
        }
    }

    // ---- header: local ⇄ remote
    Column {
        anchors.fill: parent
        Item {
            width: parent.width; height: 104
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Kiki.Theme.line }
            Row {
                anchors.centerIn: parent; spacing: 24
                Column { width: 340; spacing: 4
                    Icon { anchors.right: parent.right; name: "hdd"; size: 36; strokeWidth: 1; color: Kiki.Theme.fgDim }
                    Text { anchors.right: parent.right; text: "local"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                    Text { anchors.right: parent.right; text: Kiki.Format.display(ws.localUri, ws.home); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                }
                Row { spacing: 4; anchors.verticalCenter: parent.verticalCenter
                    Repeater { model: [{ i: "arr-l", up: false }, { i: "arr-r", up: true }]
                        delegate: Rectangle { required property var modelData; width: 32; height: 32; radius: 2; color: ws.upload === modelData.up ? Kiki.Theme.surface : "transparent"
                            Icon { anchors.centerIn: parent; name: modelData.i; size: 20; color: ws.upload === modelData.up ? Kiki.Theme.accent : Kiki.Theme.gutter }
                            MouseArea { anchors.fill: parent; enabled: ws.screen === "configure"; onClicked: ws.upload = modelData.up } } }
                }
                Column { width: 340; spacing: 4
                    Icon { name: "server"; size: 36; strokeWidth: 1; color: Kiki.Theme.green }
                    Text { text: ws.remoteUri.split("://")[1] ? ws.remoteUri.split("://")[1].split("/")[0] : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                    Text { text: Kiki.Format.display(ws.remoteUri, ws.home).replace(/^[^/]*/, ""); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                }
            }
        }
        Loader {
            width: parent.width; height: parent.height - 104 - 56
            sourceComponent: ws.screen === "configure" ? configure : (ws.screen === "preflight" ? preflight : (ws.screen === "review" ? review : running))
        }
        // ---- footer
        Item {
            width: parent.width; height: 56
            Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Kiki.Theme.line }
            Row {
                anchors.fill: parent; anchors.leftMargin: 20; anchors.rightMargin: 20; spacing: 8
                Button { visible: ws.screen === "review"; anchors.verticalCenter: parent.verticalCenter; text: "Back"; onClicked: ws.screen = "configure" }
                Text { anchors.verticalCenter: parent.verticalCenter; visible: ws.screen === "review"; text: "  " + (ws.counts.new || 0) + (ws.counts.changed || 0) + " copy · " + (ws.counts.deletes || 0) + " delete · " + Kiki.Format.bytes(ws.counts.copyBytes || 0) + " to transfer · " + (ws.counts.filtered || 0) + " filtered out"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { anchors.verticalCenter: parent.verticalCenter; visible: ws.screen === "running"; text: ws.runInfo && ws.runInfo.state === "done" ? "Mirror complete" : (ws.runInfo && ws.runInfo.state === "failed" ? "Mirror failed: " + ws.runInfo.error : "a mirror run is not undoable; re-run to converge"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Item { width: parent.width - 700; height: 1 }
                Button { visible: ws.screen !== "running" && ws.screen !== "preflight"; anchors.verticalCenter: parent.verticalCenter; text: "Cancel"; onClicked: ws.leave() }
                Button { visible: ws.screen === "preflight"; anchors.verticalCenter: parent.verticalCenter; text: "Back"; onClicked: { if (ws.scanJob) Kiki.Jobs.cancel(ws.scanJob); ws.screen = "configure" } }
                Button { visible: ws.screen === "review"; anchors.verticalCenter: parent.verticalCenter; text: "Save report…"; onClicked: Kiki.Daemon.request("MirrorReport", { job: ws.scanJob }, ok => { if (ok) Quickshell.execDetached(["sh", "-c", "printf '%s' \"$1\" > \"$HOME/kiki-mirror-report.txt\"", "sh", ok.text]) }) }
                Button { visible: ws.screen === "configure"; anchors.verticalCenter: parent.verticalCenter; text: "Preflight"; primary: true; onClicked: ws.preflight() }
                Button { visible: ws.screen === "review"; anchors.verticalCenter: parent.verticalCenter; text: "Mirror"; primary: true; onClicked: ws.mirror(false) }
                Button { visible: ws.screen === "running" && ws.runInfo && (ws.runInfo.state === "running" || ws.runInfo.state === "queued"); anchors.verticalCenter: parent.verticalCenter; text: "Cancel"; primary: true; onClicked: ws.cancelRun() }
                Button { visible: ws.screen === "running" && ws.runInfo && ws.runInfo.state !== "running" && ws.runInfo.state !== "queued"; anchors.verticalCenter: parent.verticalCenter; text: "Close"; primary: true; onClicked: ws.leave() }
            }
        }
    }

    component Check: Row {
        property bool on: false
        property string label: ""
        property bool enabled: true
        signal toggled()
        spacing: 10; height: 30
        Rectangle { width: 16; height: 16; radius: 2; anchors.verticalCenter: parent.verticalCenter; color: on ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: on ? Kiki.Theme.accent : Kiki.Theme.gutter
            Icon { visible: parent.parent.on; anchors.centerIn: parent; name: "check"; size: 10; strokeWidth: 2.5; color: Kiki.Theme.bg }
            MouseArea { anchors.fill: parent; enabled: parent.parent.enabled; onClicked: parent.parent.toggled() } }
        Text { anchors.verticalCenter: parent.verticalCenter; text: label; color: enabled ? Kiki.Theme.fg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
    }

    Component {
        id: configure
        Item {
            Column {
                anchors.horizontalCenter: parent.horizontalCenter; y: 28; width: Math.min(620, parent.width - 48); spacing: 14
                Row { id: detectRow; spacing: 12; height: 30; width: parent.width
                    Text { width: 150; anchors.verticalCenter: parent.verticalCenter; text: "Detect changes by"; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    Rectangle { width: Math.max(160, detectRow.width - 162); height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter
                        Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: ({ auto: "Automatic (size + date)", sizeMtime: "Size + modification date", sizeOnly: "Size only" })[ws.detector]; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        Icon { anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; name: "chev-d"; size: 12; color: Kiki.Theme.muted }
                        MouseArea { anchors.fill: parent; onClicked: { const o = ["auto", "sizeMtime", "sizeOnly"]; ws.detector = o[(o.indexOf(ws.detector) + 1) % o.length] } } }
                }
                Check { on: ws.deleteExtras; label: "Delete files on the destination that aren't on the source"; onToggled: ws.deleteExtras = !ws.deleteExtras }
                Check { on: ws.applyFilters; label: "Skip items matching the filter rules"; onToggled: ws.applyFilters = !ws.applyFilters }
                Row { spacing: 8
                    Check { on: ws.windowOn; label: "Only mirror files modified in the last"; onToggled: ws.windowOn = !ws.windowOn }
                    Rectangle { width: 70; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter; opacity: ws.windowOn ? 1 : 0.5
                        TextInput { anchors.fill: parent; anchors.margins: 8; clip: true; verticalAlignment: TextInput.AlignVCenter; text: ws.windowValue; enabled: ws.windowOn; inputMethodHints: Qt.ImhDigitsOnly; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; onTextChanged: ws.windowValue = parseInt(text) || 1 } }
                    Rectangle { width: 100; height: 30; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.gutter; opacity: ws.windowOn ? 1 : 0.5
                        Text { x: 10; anchors.verticalCenter: parent.verticalCenter; text: ws.windowUnit; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                        MouseArea { anchors.fill: parent; enabled: ws.windowOn; onClicked: { const o = ["hours", "days", "weeks"]; ws.windowUnit = o[(o.indexOf(ws.windowUnit) + 1) % o.length] } } }
                }
                Column { spacing: 2; width: parent.width
                    Text { text: "Modification date offset"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                    Text { text: "Determined automatically from files present on both sides"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
                Rectangle { width: parent.width; height: planText.height + 40; radius: 2; color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line
                    Column { anchors.fill: parent; anchors.margins: 14; spacing: 6
                        Text { text: "PLAN"; color: Kiki.Theme.accent; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.bold: true; font.letterSpacing: 1 }
                        Text { id: planText; width: parent.width; wrapMode: Text.WordWrap; lineHeight: 1.4; textFormat: Text.RichText; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize
                            text: "Mirror the <b style='color:" + Kiki.Theme.fg + "'>" + (ws.upload ? "local" : "remote") + "</b> folder to the <b style='color:" + Kiki.Theme.fg + "'>" + (ws.upload ? "remote" : "local") + "</b> folder. New and changed files are copied; " + (ws.deleteExtras ? "<b style='color:" + Kiki.Theme.danger + "'>files missing on the source are deleted</b>." : "nothing is deleted.") + (ws.applyFilters ? " Files matching your filter rules are ignored." : "") } }
                }
                Text { visible: ws.status !== ""; text: ws.status; color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            }
        }
    }

    Component {
        id: preflight
        Item {
            Column { anchors.centerIn: parent; spacing: 16; width: 400
                Rectangle { width: parent.width; height: 6; radius: 3; color: Kiki.Theme.surface; clip: true
                    Rectangle { id: bar; width: 120; height: 6; radius: 3; color: Kiki.Theme.accent
                        SequentialAnimation on x { loops: Animation.Infinite; NumberAnimation { from: -120; to: 400; duration: 1200 } } } }
                Text { anchors.horizontalCenter: parent.horizontalCenter; text: ws.status; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            }
        }
    }

    component PlanRow: Item {
        required property int index
        property var r: ws.plan.row(index)
        property bool running: ws.screen === "running"
        width: parent ? parent.width : 900; height: 28
        Connections { target: ws.plan; function onRowsUpdated(first, n) { if (index >= first && index < first + n) r = ws.plan.row(index) } function onReset() { r = ws.plan.row(index) } }
        Row {
            anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 12
            Rectangle { visible: !running; width: 16; height: 16; radius: 2; anchors.verticalCenter: parent.verticalCenter; opacity: r && r.action === "skip" ? 0 : 1
                color: r && r.checked ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: r && r.checked ? Kiki.Theme.accent : Kiki.Theme.gutter
                Icon { visible: r && r.checked; anchors.centerIn: parent; name: "check"; size: 10; strokeWidth: 2.5; color: Kiki.Theme.bg }
                MouseArea { anchors.fill: parent; enabled: r && r.action !== "skip"; onClicked: ws.toggleRow(index, !r.checked) } }
            Row { width: Math.min(150, Math.floor(parent.width * 0.3)); spacing: 6; anchors.verticalCenter: parent.verticalCenter; clip: true
                Icon { anchors.verticalCenter: parent.verticalCenter; size: 12; name: r ? (r.action === "delete" || r.action === "rmdir" ? "x" : (r.action === "skip" ? "equals" : (ws.upload ? "arr-u" : "arr-dn"))) : "equals"; color: r ? (r.action === "delete" || r.action === "rmdir" ? Kiki.Theme.danger : (r.reason === "changed" ? Kiki.Theme.yellow : (r.action === "skip" ? Kiki.Theme.gutter : Kiki.Theme.accent))) : Kiki.Theme.gutter }
                Text { text: r ? (r.action === "copy" ? "copy (" + r.reason + ")" : (r.action === "skip" ? "unchanged" : r.action)) : ""; color: r && (r.action === "delete" || r.action === "rmdir") ? Kiki.Theme.danger : (r && r.action === "skip" ? Kiki.Theme.muted : Kiki.Theme.fgDim); font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize } }
            Text { width: Math.max(60, parent.width - 16 - 12 - Math.min(150, Math.floor(parent.width * 0.3)) - 12 - 90 - (running ? 232 : 0)); anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideMiddle; text: r ? r.rel : ""; color: r && r.action === "skip" ? Kiki.Theme.muted : Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Text { width: 90; anchors.verticalCenter: parent.verticalCenter; horizontalAlignment: Text.AlignRight; text: r && r.bytes ? Kiki.Format.bytes(r.bytes) : "—"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Row { visible: running; width: 220; spacing: 6; anchors.verticalCenter: parent.verticalCenter
                Icon { visible: r && r.state === "done"; name: "check"; size: 14; color: Kiki.Theme.green; anchors.verticalCenter: parent.verticalCenter }
                Icon { visible: r && r.state === "skipped"; name: "warn"; size: 14; color: Kiki.Theme.yellow; anchors.verticalCenter: parent.verticalCenter }
                Rectangle { visible: r && r.state === "running"; width: 120; height: 4; radius: 2; color: Kiki.Theme.surface; anchors.verticalCenter: parent.verticalCenter
                    Rectangle { height: 4; radius: 2; color: Kiki.Theme.accent; width: parent.width * 0.5; SequentialAnimation on width { loops: Animation.Infinite; NumberAnimation { from: 10; to: 120; duration: 900 } } } }
                Text { anchors.verticalCenter: parent.verticalCenter; elide: Text.ElideRight; width: 90; text: r ? (r.state === "skipped" ? "skipped · " + (r.error || "") : (r.state === "pending" ? "queued" : r.state)) : ""; color: r && r.state === "done" ? Kiki.Theme.green : (r && r.state === "skipped" ? Kiki.Theme.yellow : (r && r.state === "pending" ? Kiki.Theme.gutter : Kiki.Theme.fgDim)); font.family: Kiki.Theme.mono; font.pixelSize: 12 } }
        }
    }

    Component {
        id: review
        Column {
            Row { height: 36; spacing: 20; x: 20
                Repeater { model: [{ id: "all", l: "All" }, { id: "new", l: "New" }, { id: "changed", l: "Changed" }, { id: "equal", l: "Unchanged" }, { id: "delete", l: "Delete" }]
                    delegate: Item { required property var modelData; width: tl.implicitWidth + 24; height: 36
                        Row { id: tl; anchors.centerIn: parent; spacing: 6
                            Text { text: modelData.l; color: ws.reviewTab === modelData.id ? Kiki.Theme.fg : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                            Text { text: modelData.id === "all" ? ws.plan.count : (modelData.id === "delete" ? (ws.counts.deletes || 0) : (ws.counts[modelData.id] || 0)); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; anchors.verticalCenter: parent.verticalCenter } }
                        Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: ws.reviewTab === modelData.id ? Kiki.Theme.accent : "transparent" }
                        MouseArea { anchors.fill: parent; onClicked: ws.setTab(modelData.id) } } }
            }
            Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
            ListView { width: parent.width; height: parent.height - 37; clip: true; reuseItems: true; model: ws.plan.count
                NaturalScroll { }
                onContentYChanged: ws.plan.setViewport(Math.max(0, Math.floor(contentY / 28)), Math.ceil(height / 28) + 1)
                Connections { target: ws.plan; function onReset() { parent.forceLayout() } }
                delegate: PlanRow {} }
        }
    }

    Component {
        id: running
        Column {
            Column { width: parent.width; spacing: 10; padding: 0
                Item { width: parent.width; height: 52
                    Row { anchors.fill: parent; anchors.leftMargin: 20; anchors.rightMargin: 20; spacing: 12
                        Text { anchors.verticalCenter: parent.verticalCenter; text: "Mirroring " + (ws.upload ? "local to " + (ws.remoteUri.split("://")[1] || "").split("/")[0] : (ws.remoteUri.split("://")[1] || "").split("/")[0] + " to local"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                        Item { width: parent.width - 620; height: 1 }
                        Text { anchors.verticalCenter: parent.verticalCenter; text: ws.runInfo ? ws.runInfo.done + " of " + ws.runInfo.total + " items · " + Kiki.Format.bytes(ws.runInfo.bytes) + " of " + Kiki.Format.bytes(ws.runInfo.bytesTotal) : ""; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                        Rectangle { anchors.verticalCenter: parent.verticalCenter; height: 22; width: 100; radius: 2; border.width: 1; border.color: Kiki.Theme.gutter; color: "transparent"
                            Text { anchors.centerIn: parent; text: ws.concurrency + " at a time"; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
                    }
                }
                Rectangle { width: parent.width - 40; x: 20; height: 6; radius: 3; color: Kiki.Theme.surface
                    Rectangle { height: 6; radius: 3; color: Kiki.Theme.accent; width: ws.runInfo && ws.runInfo.total ? parent.width * ws.runInfo.done / ws.runInfo.total : 0 } }
                Rectangle { width: parent.width; height: 1; color: Kiki.Theme.line }
            }
            ListView { width: parent.width; height: parent.height - 80; clip: true; reuseItems: true; model: ws.plan.count
                NaturalScroll { }
                onContentYChanged: ws.plan.setViewport(Math.max(0, Math.floor(contentY / 28)), Math.ceil(height / 28) + 1)
                Connections { target: ws.plan; function onReset() { parent.forceLayout() } }
                delegate: PlanRow {} }
        }
    }

    // Blast-radius confirmation
    Rectangle {
        id: confirmBox
        visible: false; anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 20
        MouseArea { anchors.fill: parent }
        Rectangle { anchors.centerIn: parent; width: 460; height: 160; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.danger
            Column { anchors.fill: parent; anchors.margins: 20; spacing: 14
                Text { text: "Large delete"; color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
                Text { width: parent.width; wrapMode: Text.WordWrap; text: "This will delete " + (ws.counts.deletes || 0) + " of " + (ws.counts.replicaEntries || 0) + " items (" + Math.round(100 * (ws.counts.deletes || 0) / Math.max(1, ws.counts.replicaEntries || 1)) + "%) on the destination. Proceed?"; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                Row { spacing: 8; anchors.right: parent.right
                    Button { text: "Cancel"; onClicked: confirmBox.visible = false }
                    Button { text: "Delete and mirror"; primary: true; onClicked: { confirmBox.visible = false; ws.mirror(true) } } }
            }
        }
    }
}
