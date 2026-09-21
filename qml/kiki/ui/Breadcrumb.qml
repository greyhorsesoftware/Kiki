import QtQuick
import Quickshell
import ".." as Kiki

// The pane URI as crumbs. Click a crumb to jump to it, click the path itself (or Ctrl+L) to
// type one, and right-click for the folders above this one.
Rectangle {
    id: bc
    property string uri: ""
    property string home: ""
    property bool editing: false
    property var repo: null
    signal navigate(string uri)
    signal pathMenu()
    height: 30; radius: 2
    color: editing ? Kiki.Theme.bgDark : "transparent"
    clip: true
    // What the crumbs may use: the field less the branch chip.
    readonly property int crumbSpace: width - 20 - (branchChip.visible ? branchChip.width + 10 : 0)

    function edit() { editing = true; input.text = bc.uri; input.forceActiveFocus(); input.selectAll() }
    /// The folders above this one, nearest first: what the path dropdown offers.
    function ancestors() {
        const parts = Kiki.Format.crumbs(uri, home)
        const out = []
        for (let i = parts.length - 2; i >= 0; i--) out.push({ label: parts[i], uri: crumbUri(i) })
        return out
    }
    function crumbUri(index) {
        const parts = Kiki.Format.crumbs(uri, home)
        if (uri.startsWith("file://")) {
            let path = parts[0] === "/" ? "" : (parts[0] === "~" ? home : "")
            const start = (parts[0] === "/" || parts[0] === "~") ? 1 : 0
            for (let i = start; i <= index; i++) path += "/" + parts[i]
            if (index === 0 && parts[0] === "/") path = "/"
            return "file://" + encodeURI(path || "/")
        }
        const i = uri.indexOf("://"); const auth = parts[0]
        let path = ""; for (let k = 1; k <= index; k++) path += "/" + parts[k]
        return uri.slice(0, i + 3) + auth + (path || "/")
    }

    // A path too long for the field slides left so the folder you are in stays visible — and the
    // wheel, or two fingers on a touchpad, slides it back and forth to reach the rest. `scroll`
    // is how far it has been pulled back towards its start: 0 at rest, `overflow` with the first
    // crumb against the left edge.
    readonly property real overflow: Math.max(0, crumbRow.width + 10 - crumbSpace)
    property real scroll: 0
    onUriChanged: scroll = 0                     // a new folder shows its own end
    onOverflowChanged: scroll = Math.max(0, Math.min(scroll, overflow))
    function scrollBy(d) { scroll = Math.max(0, Math.min(overflow, scroll + d)) }
    // A WheelHandler hears one axis, so there are two. A touchpad gives pixels; a mouse wheel
    // gives notches, and only turns one way — so its vertical turn is taken as sideways here,
    // there being nothing vertical to scroll.
    function wheeled(event, horizontal) {
        const px = horizontal ? event.pixelDelta.x : event.pixelDelta.y
        const d = px !== 0 ? px : (horizontal ? event.angleDelta.x : event.angleDelta.y) / 2
        if (d === 0) { event.accepted = false; return }
        bc.scrollBy(d)
        event.accepted = true
    }
    WheelHandler {
        enabled: !bc.editing && bc.overflow > 0
        orientation: Qt.Vertical
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onWheel: event => bc.wheeled(event, false)
    }
    WheelHandler {
        enabled: !bc.editing && bc.overflow > 0
        orientation: Qt.Horizontal
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onWheel: event => bc.wheeled(event, true)
    }
    // The crumbs have the field less the branch chip, and are clipped to it: scrolled back, the
    // row must slide under the chip's edge rather than over the chip.
    Item {
        id: crumbView
        objectName: "crumb-view"
        visible: !bc.editing
        width: Math.max(0, bc.crumbSpace + 10); height: parent.height
        clip: true
    Row {
        id: crumbRow
        objectName: "crumb-row"
        anchors.verticalCenter: parent.verticalCenter; spacing: 6
        x: Math.min(10, bc.crumbSpace - width) + bc.scroll
        Repeater {
            model: Kiki.Format.crumbs(bc.uri, bc.home)
            delegate: Row {
                required property int index
                required property string modelData
                spacing: 6
                Icon { visible: index > 0; name: "chev-r"; size: 12; color: Kiki.Theme.gutter; anchors.verticalCenter: parent.verticalCenter }
                // Every crumb is a chip; home shows its icon instead of a "~".
                Rectangle {
                    id: crumbChip
                    readonly property bool isHome: index === 0 && modelData === "~"
                    readonly property bool last: index === Kiki.Format.crumbs(bc.uri, bc.home).length - 1
                    anchors.verticalCenter: parent.verticalCenter
                    width: isHome ? 28 : crumbText.implicitWidth + 20
                    height: 22; radius: 11
                    color: hit.containsMouse ? Kiki.Theme.accent : Kiki.Theme.surface
                    Icon { visible: crumbChip.isHome; anchors.centerIn: parent; name: "home"; size: 13; color: hit.containsMouse ? Kiki.Theme.bg : Kiki.Theme.chrome }
                    Text {
                        id: crumbText
                        visible: !crumbChip.isHome
                        anchors.centerIn: parent
                        text: modelData
                        color: hit.containsMouse ? Kiki.Theme.bg : (crumbChip.last ? Kiki.Theme.fg : Kiki.Theme.muted)
                        font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: crumbChip.last
                    }
                    MouseArea { id: hit; anchors.fill: parent; hoverEnabled: true; onClicked: bc.navigate(bc.crumbUri(index)) }
                }
            }
        }
    }
    }
    // Branch chip (plan 15)
    Rectangle {
        id: branchChip
        visible: !bc.editing && bc.repo && bc.repo.branch
        anchors.right: parent.right; anchors.rightMargin: 6; anchors.verticalCenter: parent.verticalCenter
        height: 20; width: chip.width + 14; radius: 2; color: Kiki.Theme.surface
        Row { id: chip; anchors.centerIn: parent; spacing: 5
            Icon { name: "mirror"; size: 10; color: bc.repo && bc.repo.dirty ? Kiki.Theme.yellow : Kiki.Theme.muted; anchors.verticalCenter: parent.verticalCenter }
            Text { text: bc.repo ? (bc.repo.detached ? bc.repo.branch : bc.repo.branch) + (bc.repo.ahead ? " ↑" + bc.repo.ahead : "") + (bc.repo.behind ? " ↓" + bc.repo.behind : "") : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: 11 } }
        MouseArea { anchors.fill: parent; onClicked: Quickshell.execDetached(["wl-copy", bc.repo.branch]) }
    }
    TextInput {
        id: input
        visible: bc.editing
        anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 10; verticalAlignment: TextInput.AlignVCenter
        color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; selectionColor: Kiki.Theme.accent; clip: true
        onAccepted: { bc.editing = false; bc.navigate(text.startsWith("/") || text.startsWith("~") ? "file://" + encodeURI(text.replace(/^~/, bc.home)) : text) }
        Keys.onEscapePressed: bc.editing = false
        onActiveFocusChanged: if (!activeFocus) bc.editing = false
    }
    // A press anywhere else does what Escape does: the field goes and the crumbs come back. The
    // field only went when it lost the keyboard focus, and a click on the files takes no focus
    // from anything — so it stayed until Escape. Over the whole window while the field is up,
    // and it lets the press through: the row that was clicked is still clicked.
    MouseArea {
        objectName: "breadcrumb-outside"
        parent: bc.Window.contentItem ? bc.Window.contentItem : bc
        anchors.fill: parent; z: 1000
        enabled: bc.editing; visible: bc.editing
        acceptedButtons: Qt.AllButtons
        onPressed: mouse => {
            const p = mapToItem(bc, mouse.x, mouse.y)
            if (!(p.x >= 0 && p.y >= 0 && p.x < bc.width && p.y < bc.height)) bc.editing = false
            mouse.accepted = false
        }
    }
    // Sits behind the crumbs, so clicking a crumb still jumps to it. Clicking the rest of the
    // path turns it into a field you type in; the folders above are on the right button.
    MouseArea {
        anchors.fill: parent; visible: !bc.editing; z: -1
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: mouse => mouse.button === Qt.RightButton ? bc.pathMenu() : bc.edit()
    }
}
