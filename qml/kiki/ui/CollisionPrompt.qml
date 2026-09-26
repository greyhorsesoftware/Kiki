import QtQuick
import ".." as Kiki

// Modal for a job that found an existing destination.
Rectangle {
    id: dlg
    property var prompt: Kiki.Jobs.prompt
    /// Cleared for every new prompt: "apply to all" must never carry over into the next job.
    property bool all: false
    onPromptChanged: dlg.all = false
    visible: prompt !== null
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 90
    MouseArea { anchors.fill: parent }   // swallow clicks
    Rectangle {
        anchors.centerIn: parent; width: 480; height: 220; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            anchors.fill: parent; anchors.margins: 20; spacing: 14
            Text { text: Kiki.T.tr("collision.title"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            Text { width: parent.width; wrapMode: Text.WrapAnywhere; text: dlg.prompt ? decodeURIComponent(dlg.prompt.uri.split("/").pop()) : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Grid {
                columns: 3; columnSpacing: 12; rowSpacing: 4
                Text { text: ""; font.pixelSize: 11 } Text { text: Kiki.T.tr("collision.existing"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } Text { text: Kiki.T.tr("collision.incoming"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                Text { text: Kiki.T.tr("collision.size"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                Text { text: dlg.prompt ? Kiki.Format.bytes(dlg.prompt.existing.size) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { text: dlg.prompt ? Kiki.Format.bytes(dlg.prompt.incoming.size) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { text: Kiki.T.tr("collision.modified"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                Text { text: dlg.prompt ? Kiki.Format.date(dlg.prompt.existing.mtime) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { text: dlg.prompt ? Kiki.Format.date(dlg.prompt.incoming.mtime) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            }
            Row {
                spacing: 8
                Rectangle { objectName: "collision-all"; width: 16; height: 16; radius: 2; anchors.verticalCenter: parent.verticalCenter; color: dlg.all ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: dlg.all ? Kiki.Theme.accent : Kiki.Theme.gutter; MouseArea { anchors.fill: parent; onClicked: dlg.all = !dlg.all } }
                Text { anchors.verticalCenter: parent.verticalCenter; text: Kiki.T.tr("collision.applyAll"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Item { width: 40; height: 1 }
                Button { objectName: "collision-skip"; text: Kiki.T.tr("collision.skip"); onClicked: Kiki.Jobs.reply("skip", dlg.all) }
                Button { objectName: "collision-keepBoth"; text: Kiki.T.tr("collision.keepBoth"); onClicked: Kiki.Jobs.reply("keepBoth", dlg.all) }
                Button { objectName: "collision-replace"; text: Kiki.T.tr("collision.replace"); primary: true; onClicked: Kiki.Jobs.reply("replace", dlg.all) }
            }
        }
    }
}
