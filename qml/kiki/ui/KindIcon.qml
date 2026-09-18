import QtQuick
import ".." as Kiki

// The icon for a file kind: kiki's own drawing, or the desktop's icon theme when the setting
// says so.
//
// The theme icon is resolved by the daemon rather than by Qt. Qt answers with a provider URL
// that is the same string whatever the theme, so a theme switch leaves the old picture in the
// image cache until the delegate is rebuilt; a file path changes when the theme does.
Item {
    id: ki
    property string kind: "file"
    property int size: 16
    property color color: Kiki.Theme.kindColor(kind)

    implicitWidth: size; implicitHeight: size

    /// kiki's kinds mapped onto the freedesktop names an icon theme actually ships.
    readonly property var themeNames: ({
        folder: "folder", image: "image-x-generic", video: "video-x-generic", audio: "audio-x-generic",
        code: "text-x-script", text: "text-x-generic", document: "x-office-document", pdf: "application-pdf",
        archive: "package-x-generic", link: "emblem-symbolic-link", app: "application-x-executable",
        file: "text-x-generic"
    })
    readonly property bool useTheme: Kiki.Settings.view.icons === "system" && Kiki.Theme.iconTheme !== ""

    property string path: ""
    /// One request per kind, size and theme; the daemon remembers the answer.
    function resolve() {
        if (!useTheme) { path = ""; return }
        const want = themeNames[kind] || "text-x-generic"
        const theme = Kiki.Theme.iconTheme
        Kiki.Daemon.request("Icon", { name: want, theme: theme, size: Math.max(16, ki.size) }, (ok, err) => {
            // A late answer for a theme we have already left is not ours to show.
            if (theme !== Kiki.Theme.iconTheme) return
            ki.path = ok && ok.path ? "file://" + ok.path : ""
        })
    }
    Component.onCompleted: resolve()
    onKindChanged: resolve()
    onSizeChanged: resolve()
    Connections {
        target: Kiki.Theme
        function onIconThemeChanged() { ki.resolve() }
    }
    Connections {
        target: Kiki.Settings
        function onViewChanged() { ki.resolve() }
    }

    Icon {
        anchors.fill: parent
        visible: ki.path === ""
        name: ki.kind; size: ki.size; color: ki.color
    }
    Image {
        anchors.fill: parent
        visible: ki.path !== ""
        source: ki.path
        sourceSize: Qt.size(ki.size * 2, ki.size * 2)
        fillMode: Image.PreserveAspectFit
        smooth: true; asynchronous: true; cache: false
    }
}
