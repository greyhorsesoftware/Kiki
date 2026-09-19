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
    /// The theme's file for this kind. Answered from `Theme.iconPath` on the spot when that name
    /// has been resolved before, so a rebuilt delegate draws the right icon in its first frame
    /// rather than changing under the pointer a moment later.
    function resolve() {
        if (!useTheme) { path = ""; return }
        const want = themeNames[kind] || "text-x-generic"
        ki.path = Kiki.Theme.iconPath(want, Math.max(16, ki.size), p => {
            // The delegate may have been destroyed, or handed a different row, while the answer
            // was in flight.
            if (!ki) return
            if (want === (ki.themeNames[ki.kind] || "text-x-generic")) ki.path = p
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
        // An unbound delegate (the row it will show has not arrived) draws nothing: guessing a
        // kind here means every rebuilt row changes picture a frame later.
        visible: ki.kind !== "" && ki.path === ""
        name: ki.kind; size: ki.size; color: ki.color
    }
    Image {
        anchors.fill: parent
        visible: ki.kind !== "" && ki.path !== ""
        source: ki.path
        sourceSize: Qt.size(ki.size * 2, ki.size * 2)
        fillMode: Image.PreserveAspectFit
        smooth: true; asynchronous: true; cache: false
    }
}
