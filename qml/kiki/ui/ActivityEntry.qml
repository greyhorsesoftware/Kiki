import QtQuick
import ".." as Kiki

// One job in the activity popup: what it is, how far along, and what came of it (plan 32).
Item {
    id: e
    property var job: ({})
    property bool first: false
    property bool expanded: false
    signal toggleDetail()
    signal log()
    signal reveal()

    readonly property bool live: Kiki.Jobs.live(job)
    readonly property string mode: Kiki.Jobs.barMode(job)
    readonly property bool detailAvailable: Kiki.Jobs.hasDetail(job)
    objectName: "activity-entry-" + job.id
    height: body.height + 20

    Rectangle { visible: !e.first; width: parent.width - 24; x: 12; height: 1; color: Kiki.Theme.line }

    KindIcon {
        x: 14; y: 12; size: 32
        kind: e.job.op === "mirrorRun" ? "folder" : (e.job.isDir ? "folder" : "file")
        color: Kiki.Theme.kindColor(e.job.isDir || e.job.op === "mirrorRun" ? "folder" : "file")
    }

    Column {
        id: body
        x: 58; y: 10; width: e.width - 58 - buttons.width - 14; spacing: 5
        Text {
            objectName: "activity-headline"
            width: parent.width; elide: Text.ElideMiddle
            text: Kiki.Jobs.headline(e.job)
            color: e.job.state === "failed" ? Kiki.Theme.danger : Kiki.Theme.fg
            font.family: Kiki.Theme.mono; font.pixelSize: 12; font.bold: true
        }
        // While it runs: a thin bar, with no length until there are real numbers.
        Rectangle {
            objectName: "activity-bar"
            visible: e.mode !== "none"
            width: parent.width; height: 5; radius: 2.5; color: Kiki.Theme.surface; clip: true
            Rectangle {
                visible: e.mode === "value"
                height: parent.height; radius: 2.5; color: Kiki.Theme.accent
                width: Math.round(parent.width * Kiki.Jobs.fraction(e.job))
            }
            Rectangle {
                id: runner
                visible: e.mode === "busy"
                width: parent.width * 0.3; height: parent.height; radius: 2.5; color: Kiki.Theme.accent; opacity: 0.8
                SequentialAnimation on x {
                    running: runner.visible; loops: Animation.Infinite
                    NumberAnimation { from: -runner.width; to: runner.parent.width; duration: 1400; easing.type: Easing.InOutSine }
                }
            }
        }
        Row {
            visible: e.live && statusText.text !== ""
            width: parent.width; spacing: 4
            Text {
                objectName: "activity-disclosure"
                visible: e.detailAvailable
                text: e.expanded ? "▼" : "▶"; color: Kiki.Theme.muted; font.pixelSize: 9; anchors.verticalCenter: parent.verticalCenter
                MouseArea { anchors.fill: parent; anchors.margins: -6; cursorShape: Qt.PointingHandCursor; onClicked: e.toggleDetail() }
            }
            Text {
                id: statusText
                objectName: "activity-status"
                width: parent.width - 14; elide: Text.ElideRight
                text: Kiki.Jobs.statusLine(e.job); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
        }
        // The file in hand: its name, its own bar, its own numbers.
        Column {
            objectName: "activity-detail"
            visible: e.detailAvailable && e.expanded
            x: 12; width: parent.width - 12; spacing: 4
            Text { width: parent.width; elide: Text.ElideMiddle; text: e.job.current ? e.job.current.name : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
            Rectangle {
                visible: !!e.job.current && e.job.current.size > 0
                width: parent.width; height: 4; radius: 2; color: Kiki.Theme.surface
                Rectangle { height: 4; radius: 2; color: Kiki.Theme.accent; width: e.job.current && e.job.current.size ? Math.round(parent.width * Math.min(1, e.job.current.bytes / e.job.current.size)) : 0 }
            }
            Text { visible: text !== ""; text: Kiki.Jobs.detailLine(e.job); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 10 }
        }
        // Once it is over: what came of it, in words that wrap.
        Text {
            objectName: "activity-completion"
            visible: !e.live
            width: parent.width; wrapMode: Text.Wrap
            text: e.live ? "" : Kiki.Jobs.completion(e.job) + (e.job.state === "failed" ? " — see Log" : "")
            color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
        }
    }

    Row {
        id: buttons
        anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter; spacing: 2
        RoundButton_ { objectName: "activity-log"; icon: "doc"; tip: "Log"; onClicked: e.log() }
        RoundButton_ { objectName: "activity-cancel"; visible: e.live; enabled: !e.job.cancelling; icon: "x"; tip: "Cancel"; danger: true; onClicked: Kiki.Jobs.cancel(e.job.id) }
        RoundButton_ { objectName: "activity-reveal"; visible: !e.live && !!e.job.revealUri; icon: "folder"; tip: "Reveal"; onClicked: e.reveal() }
        RoundButton_ { objectName: "activity-dismiss"; visible: !e.live && !e.job.revealUri; icon: "x"; tip: "Dismiss"; onClicked: Kiki.Jobs.dismiss(e.job.id) }
    }

    component RoundButton_: Item {
        id: rb
        property string icon: ""
        property string tip: ""
        property bool danger: false
        property bool enabled: true
        signal clicked()
        width: 32; height: 32; opacity: enabled ? 1 : 0.4
        Rectangle { anchors.fill: parent; radius: 16; color: rbArea.containsMouse && rb.enabled ? Kiki.Theme.surface : "transparent" }
        Icon { anchors.centerIn: parent; name: rb.icon; size: 14; color: rbArea.containsMouse && rb.danger ? Kiki.Theme.danger : Kiki.Theme.fgDim }
        Tip { visible: rbArea.containsMouse; text: rb.tip }
        MouseArea { id: rbArea; anchors.fill: parent; hoverEnabled: true; enabled: rb.enabled; cursorShape: Qt.PointingHandCursor; onClicked: rb.clicked() }
    }
}
