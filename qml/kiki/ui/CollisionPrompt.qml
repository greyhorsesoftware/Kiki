import QtQuick
import ".." as Kiki

// Modal for a job that found an existing destination.
Rectangle {
    id: dlg
    property var prompt: Kiki.Jobs.prompt
    visible: prompt !== null
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 90
    MouseArea { anchors.fill: parent }   // swallow clicks
    Rectangle {
        anchors.centerIn: parent; width: 480; height: 220; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        Column {
            anchors.fill: parent; anchors.margins: 20; spacing: 14
            Text { text: "Already exists"; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            Text { width: parent.width; wrapMode: Text.WrapAnywhere; text: dlg.prompt ? decodeURIComponent(dlg.prompt.uri.split("/").pop()) : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
            Grid {
                columns: 3; columnSpacing: 12; rowSpacing: 4
                Text { text: ""; font.pixelSize: 11 } Text { text: "existing"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 } Text { text: "incoming"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                Text { text: "size"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                Text { text: dlg.prompt ? Kiki.Format.bytes(dlg.prompt.existing.size) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { text: dlg.prompt ? Kiki.Format.bytes(dlg.prompt.incoming.size) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { text: "modified"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11 }
                Text { text: dlg.prompt ? Kiki.Format.date(dlg.prompt.existing.mtime) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Text { text: dlg.prompt ? Kiki.Format.date(dlg.prompt.incoming.mtime) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
            }
            Row {
                spacing: 8
                property bool all: false
                Rectangle { width: 16; height: 16; radius: 2; anchors.verticalCenter: parent.verticalCenter; color: parent.all ? Kiki.Theme.accent : Kiki.Theme.bgDark; border.width: 1; border.color: parent.all ? Kiki.Theme.accent : Kiki.Theme.gutter; MouseArea { anchors.fill: parent; onClicked: parent.parent.all = !parent.parent.all } }
                Text { anchors.verticalCenter: parent.verticalCenter; text: "Apply to all"; color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 12 }
                Item { width: 40; height: 1 }
                Button { text: "Skip"; onClicked: Kiki.Jobs.reply("skip", parent.all) }
                Button { text: "Keep both"; onClicked: Kiki.Jobs.reply("keepBoth", parent.all) }
                Button { text: "Replace"; primary: true; onClicked: Kiki.Jobs.reply("replace", parent.all) }
            }
        }
    }
}
