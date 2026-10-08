import QtQuick
import ".." as Kiki

// First launch (plan 09): offer to make kiki the default for this user. One answer, not four —
// the list says what it does in a person's words, and Settings → Omarchy is one switch over the
// same list (owner, 2026-10-06: "just one switch on/off w/ list of stuff we bind (in human
// terms) no file nonsense" … "yes do the dialog too"). It is all of it or none: a desktop where
// the keys are kiki's but the Open dialog is not is a desktop nobody asked for, and every part
// is per-user and reversible from that switch.
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.55); z: 95
    property var status: ({})
    property var results: []
    property bool busy: false
    /// What did not take, in the same words the list uses — "" when nothing failed.
    property string trouble: ""
    function open() { results = []; trouble = ""; Kiki.Daemon.request("Integration", {}, ok => { if (ok) status = ok; visible = true }) }
    function decide(apply) {
        Kiki.Settings.set("integration", "asked", true)
        if (!apply) { visible = false; return }
        busy = true
        // Every part: the dialog no longer asks which, so it sends no `parts` and the daemon
        // does the lot.
        Kiki.Daemon.request("Integrate", {}, (ok, err) => {
            busy = false
            if (err) { trouble = err.message; return }
            results = ok.results; status = ok.status
            const failed = (ok.results || []).filter(r => !r.ok).map(r => Kiki.T.tr("settings.omarchyPart." + r.part))
            trouble = failed.length ? Kiki.T.tr("settings.omarchyTrouble", { which: failed.join(", ") }) : ""
            if (!failed.length) visible = false
        })
    }
    MouseArea { anchors.fill: parent }
    Rectangle {
        // Never wider than the window it is asked in: at 640 flat it ran off the edges of a
        // half-width kiki (owner, 2026-09-26). The same rule the settings pages keep.
        anchors.centerIn: parent; width: Math.min(640, dlg.width - 48); height: col.height + 48; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            id: col; x: 24; y: 24; width: parent.width - 48; spacing: 12
            Text { text: Kiki.T.tr("integration.title"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 16; font.bold: true }
            Text { width: parent.width; wrapMode: Text.WordWrap; text: Kiki.T.tr("settings.omarchyWhatItDoes"); color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            Repeater {
                model: Kiki.T.omarchyDoes()
                delegate: Text {
                    required property var modelData
                    width: col.width; wrapMode: Text.WordWrap
                    text: "·  " + modelData; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12
                }
            }
            Text { width: parent.width; wrapMode: Text.WordWrap; text: Kiki.T.tr("integration.note"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { objectName: "integration-trouble"; visible: dlg.trouble !== ""; width: parent.width; wrapMode: Text.WordWrap; text: dlg.trouble; color: Kiki.Theme.danger; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Text { visible: !!dlg.status.hyprConfigErrors && dlg.status.hyprConfigErrors.length > 0; width: parent.width; wrapMode: Text.WordWrap; text: Kiki.T.tr("integration.hyprErrors", { errors: (dlg.status.hyprConfigErrors || []).join("\n") }); color: Kiki.Theme.yellow; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Row {
                spacing: 8; anchors.right: parent.right
                Button { text: Kiki.T.tr("integration.notNow"); onClicked: dlg.decide(false) }
                Button { objectName: "integration-yes"; text: dlg.busy ? Kiki.T.tr("integration.applying") : Kiki.T.tr("integration.makeDefault"); primary: true; enabled: !dlg.busy; onClicked: dlg.decide(true) }
            }
        }
    }
}
