import QtQuick
import Quickshell
import ".." as Kiki

// Add or edit a location. One tab per installed plugin; fields from its Describe form.
Rectangle {
    id: dlg
    visible: false
    anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.5); z: 90
    property var plugins: []          // Describe results
    property int tab: 0
    property var values: ({})         // key -> string
    property var errors: ({})         // key -> message
    property string status: ""
    property bool busy: false
    property string editingName: ""   // non-empty when editing
    /// Set when the server offered a key nobody has accepted yet: until the user says yes, the
    /// location is not saved and nothing is connected to.
    property string verifyFingerprint: ""
    property string verifyHost: ""
    signal saved()
    /// "Add and Connect": the location called `name` was saved and should now be opened.
    signal openRequested(string name)
    /// The local path's folder icon was clicked: the window shows its folder chooser starting at
    /// `start` (a path, may be empty) and calls `reply(path)` with what was chosen. Not called
    /// at all when the chooser is cancelled.
    signal chooseFolder(string start, var reply)
    /// The picture this location wears in the sidebar — a path, "" for the plain glyph — and the
    /// request for a chooser to pick one (`reply(path)`), answered by the shell like the above.
    property string image: ""
    signal chooseImage(string start, var reply)

    /// Open a saved location that was added without being checked and go straight to
    /// "Save and Connect": the daemon refused to connect because nobody has seen its server's
    /// key, and this is where it gets seen.
    property bool _verifyWhenLoaded: false
    /// Verifying a saved location's server is a question, not an edit: only the question is
    /// shown — no form behind it — and whatever the answer, that is the end of it.
    property bool verifyOnly: false
    function verify(existing) { _verifyWhenLoaded = true; verifyOnly = true; open(existing) }
    /// The end of a verify-only visit that did not save: say why, since there is no form to say it in.
    function _leaveVerify(why) { verifyOnly = false; verifyFingerprint = ""; visible = false; if (why) Kiki.Jobs.showToast({ text: why, undoable: false }) }
    function open(existing) {
        if (!_verifyWhenLoaded) verifyOnly = false
        openAfter = false
        errors = ({}); status = ""; busy = false; verifyFingerprint = ""; verifyHost = ""
        image = existing && existing.image ? existing.image : ""
        Kiki.Daemon.request("Plugins", {}, ok => {
            if (!ok) return
            plugins = ok.plugins
            if (existing) {
                editingName = existing.name
                tab = Math.max(0, plugins.findIndex(p => p.scheme === existing.plugin))
                const v = Object.assign({}, existing.config || {})
                v.name = existing.name
                v.remotePath = uriPath(existing.remoteUri); v.localPath = existing.localUri ? decodeURIComponent(existing.localUri.replace(/^file:\/\//, "")) : ""
                values = v
                if (dlg._verifyWhenLoaded) { dlg._verifyWhenLoaded = false; dlg.connect(undefined, true) }
            } else {
                editingName = ""; tab = 0; values = defaults(plugins[0])
            }
            visible = true
        })
    }
    function defaults(p) { const v = {}; for (const f of (p ? p.form : [])) v[f.key] = (p.defaults && p.defaults[f.key]) || f.default || ""; return v }
    function uriPath(u) { if (!u) return "/"; const i = u.indexOf("://"); const rest = u.slice(i + 3); const s = rest.indexOf("/"); return s < 0 ? "/" : decodeURIComponent(rest.slice(s)) }
    function current() { return plugins[tab] }
    /// A kind whose plugin says it cannot work here — SMB without gvfs-smb, say — shows why
    /// instead of a form that could only fail (plan 25). Absent means available.
    readonly property bool usable: !current() || current().available !== false
    // ---- the form as rows
    /// The tabs this form has: the distinct `group`s of its fields, in the order they appear.
    function groups() { const g = []; for (const f of (current() ? current().form : [])) if (f.group && g.indexOf(f.group) < 0) g.push(f.group); return g }
    /// The chosen tab, kept with the other values as `auth` (lower-cased) so it is saved with
    /// the location and comes back when it is edited. A form with a Key tab starts on it.
    readonly property string authGroup: {
        const g = groups(); if (!g.length) return ""
        const want = (values.auth || "").toLowerCase()
        return g.find(x => x.toLowerCase() === want) || (g.indexOf("Key") >= 0 ? "Key" : g[0])
    }
    function chooseGroup(g) { const nv = Object.assign({}, values); nv.auth = g.toLowerCase(); values = nv; errors = ({}) }
    /// Is this field part of the LOCATION as it stands — in no group, or in the chosen one? This
    /// is what decides whether it is validated, saved and sent.
    function shown(f) { return !f.group || f.group === authGroup }
    /// The form's pages ("Connection", "Locations"): sections shown one at a time, all of them
    /// part of the location whichever is on screen. A field with no page is on the first.
    function pages() { const p = []; for (const f of (current() ? current().form : [])) if (f.page && p.indexOf(f.page) < 0) p.push(f.page); return p.length ? ["Connection"].concat(p) : [] }
    property string page: "Connection"
    function pageOf(f) { return f.page || "Connection" }
    /// What the form draws, top to bottom: `{ fields: [...] }` rows, and one `{ tabs: [...] }`
    /// row where the first tabbed field would have been. A `port` directly after the field
    /// before it shares that field's row — a port is five digits and does not need a line.
    function formRows() {
        const rows = []; let tabbed = false
        for (const f of (current() ? current().form : [])) {
            if (pages().length && pageOf(f) !== page) continue
            if (f.group && !tabbed) { rows.push({ tabs: groups() }); tabbed = true }
            if (!shown(f)) continue
            const last = rows.length ? rows[rows.length - 1] : null
            if (f.kind === "port" && last && last.fields && last.fields.length === 1 && !last.fields[0].group) last.fields.push(f)
            else rows.push({ fields: [f] })
        }
        return rows
    }

    /// What each `keys` field's plugin found, by field key. Asked for afresh whenever the form
    /// is shown, so a key made a minute ago is there.
    property var keyChoices: ({})
    property var _keysDefaulted: ({})
    function loadKeyChoices() {
        const p = current(); if (!p) return
        for (const f of p.form) {
            if (f.kind !== "keys") continue
            const key = f.key, scheme = p.scheme
            Kiki.Daemon.request("PluginBrowse", { plugin: scheme, field: key, config: {}, secrets: {} }, (ok, err) => {
                if (!dlg.current() || dlg.current().scheme !== scheme) return
                const found = ok ? (ok.options || []) : []
                const kc = Object.assign({}, dlg.keyChoices); kc[key] = found; dlg.keyChoices = kc
                // A NEW location starts with the first key found ticked — the one ssh itself
                // would reach for. Once per opening, and never for a saved location being
                // edited: there, nothing ticked is what was saved (password only), and
                // unticking everything must not tick one back.
                if (!dlg.editingName && !dlg._keysDefaulted[key] && found.length && !(dlg.values[key] || "")) {
                    const nv = Object.assign({}, dlg.values); nv[key] = found[0].value; dlg.values = nv
                }
                // …and with no key to be found, on the Password tab: the Key tab would be empty.
                if (!dlg.editingName && !dlg._keysDefaulted[key] && !found.length && !(dlg.values.auth || "") && dlg.groups().indexOf("Password") >= 0) dlg.chooseGroup("Password")
                const kd = Object.assign({}, dlg._keysDefaulted); kd[key] = true; dlg._keysDefaulted = kd
            })
        }
    }
    onTabChanged: { page = "Connection"; if (visible) { _keysDefaulted = ({}); loadKeyChoices() } }
    // A `browse` field asks its plugin for choices given what is filled in so far.
    function browseField(key) {
        const p = current(); if (!p) return
        status = "Looking…"
        Kiki.Daemon.request("PluginBrowse", { plugin: p.scheme, field: key, config: values, secrets: values }, (ok, err) => {
            status = err ? err.message : ""
            if (!ok) return
            const items = (ok.options || []).map(o => ({ label: o.label || o.value, action: () => { const nv = Object.assign({}, dlg.values); nv[key] = o.value; dlg.values = nv } }))
            if (!items.length) items.push({ label: Kiki.T.tr("menu.nothingFound"), enabled: false, action: () => {} })
            browseMenu.open(items, Qt.point(80, 200))
        })
    }
    function build() {
        const p = current(); const config = {}; const secrets = {}
        if (authGroup) config.auth = authGroup.toLowerCase()
        for (const f of p.form) {
            if (["name", "remotePath", "localPath"].includes(f.key)) continue
            // The other tab's fields are not part of this location: a password typed before
            // switching to Key is neither saved nor sent.
            if (!shown(f)) continue
            if ((p.secretFields || []).includes(f.key)) { if (values[f.key]) secrets[f.key] = values[f.key] } else config[f.key] = values[f.key] || ""
        }
        const name = (values.name || "").trim()
        const location = { name: name, plugin: p.scheme, remoteUri: p.scheme + "://" + name + (values.remotePath || "/"), localUri: values.localPath ? "file://" + encodeURI(values.localPath.replace(/^~/, Quickshell.env("HOME"))) : "", config: config }
        if (image) location.image = image
        return { location: location, secrets: secrets }
    }
    /// Move to another kind, keeping it on screen. Locked while editing a saved location.
    function selectKind(i) {
        if (editingName || !plugins.length) return
        const n = Math.max(0, Math.min(plugins.length - 1, i))
        if (n === tab) return
        tab = n; values = defaults(plugins[n]); errors = ({})
        kinds.positionViewAtIndex(n, ListView.Contain)
    }
    /// The icon set has no per-protocol glyphs, so map the scheme onto the closest one.
    function kindIcon(scheme) {
        switch (scheme) {
        case "sftp": case "ftps": return "server"
        case "smb": return "cloud"
        case "mtp": case "ptp": case "afc": return "usb"
        }
        return "hdd"
    }
    function validate() {
        const e = {}
        if (!(values.name || "").trim()) e.name = "name is required"
        for (const f of current().form) if (shown(f) && f.required && !(values[f.key] || "").trim() && !e[f.key]) e[f.key] = Kiki.T.tr("location.required", { field: Kiki.T.sent(f) })
        errors = e
        const bad = current().form.find(f => e[f.key])
        if (bad && pages().length && pageOf(bad) !== page && !current().form.some(f => e[f.key] && pageOf(f) === page)) page = pageOf(bad)
        return Object.keys(e).length === 0
    }
    /// `connect()` — "Add" — saves what was typed and closes: the fields are validated, nothing
    /// is looked up or connected to. `connect(trust, true)` — "Add and Connect" — signs in first
    /// (which is where the server's key is shown and accepted), saves, and opens the location. The choice rides through the
    /// verify-the-server step in `openAfter`, so trusting a key does what you originally asked.
    property bool openAfter: false
    function connect(trust, thenOpen) {
        if (!usable) return
        if (thenOpen !== undefined) openAfter = thenOpen === true
        if (!validate()) return
        busy = true; status = trust ? Kiki.T.tr("location.saving") : (openAfter ? Kiki.T.tr("location.connecting") : Kiki.T.tr("location.saving"))
        const req = build()
        if (trust) req.trust = trust
        // "Add" takes what was typed: nothing is looked up or connected to. Only "Add and
        // Connect" (and trusting a key, which is part of it) signs in to check.
        if (!openAfter && !trust) req.check = false
        Kiki.Daemon.request(editingName ? "UpdateLocation" : "AddLocation", req, (ok, err) => {
            busy = false
            if (err && verifyOnly) { _leaveVerify(err.message); return }
            if (err) { verifyFingerprint = ""; status = ""; if (err.field) { const e = {}; e[err.field] = err.message; errors = e } else status = err.message; return }
            // The server identified itself with a key kiki has never accepted. Ask before saving.
            if (ok && ok.verify) { verifyFingerprint = ok.verify; verifyHost = ok.host || values.host || ""; status = ""; return }
            verifyFingerprint = ""; verifyOnly = false
            const name = (values.name || "").trim(), open = openAfter
            visible = false; openAfter = false; dlg.saved()
            if (open) dlg.openRequested(name)
        })
    }

    MouseArea { anchors.fill: parent }
    Rectangle {
        anchors.centerIn: parent
        objectName: "location-card"
        visible: !dlg.verifyOnly
        width: Math.min(660, parent.width - 32); height: Math.min(560, parent.height - 32)
        color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.accent
        ToggleButton {
            anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 4
            z: 2; icon: "x"; tip: Kiki.T.tr("location.closeTip")
            onClicked: dlg.visible = false
        }
        Column {
            anchors.fill: parent; anchors.margins: 24; anchors.topMargin: 20; spacing: 20
            Row {
                id: header
                width: parent.width
                Text { text: dlg.editingName ? Kiki.T.tr("location.editTitle") : Kiki.T.tr("location.addTitle"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true }
            }
            Row {
            id: body
            width: parent.width; height: Math.max(0, parent.height - header.height - footer.height - 2 * parent.spacing); spacing: 20
            // One tile per kind, down the left-hand side and centred in it: across the top they
            // took a band of the form's height, and the form is what needs the room.
            ListView {
                id: kinds
                objectName: "location-kinds"
                width: 92; height: Math.min(contentHeight, body.height)
                anchors.verticalCenter: parent.verticalCenter
                orientation: ListView.Vertical; spacing: 8; clip: true
                model: dlg.plugins
                delegate: Rectangle {
                    required property var modelData
                    required property int index
                    width: 92; height: 72; radius: 14
                    color: dlg.tab === index ? Kiki.Theme.surface : Kiki.Theme.bgDark
                    border.width: 1; border.color: dlg.tab === index ? Kiki.Theme.accent : Kiki.Theme.line
                    opacity: dlg.editingName && dlg.tab !== index ? 0.4 : 1
                    Column {
                        anchors.centerIn: parent; spacing: 6; width: parent.width - 12
                        Icon {
                            anchors.horizontalCenter: parent.horizontalCenter
                            name: dlg.kindIcon(modelData.scheme); size: 22
                            color: dlg.tab === index ? Kiki.Theme.accent : Kiki.Theme.fgDim
                        }
                        Text {
                            width: parent.width; horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap; maximumLineCount: 2; elide: Text.ElideRight
                            text: modelData.displayName
                            color: dlg.tab === index ? Kiki.Theme.fg : Kiki.Theme.muted
                            font.family: Kiki.Theme.mono; font.pixelSize: 11
                        }
                    }
                    MouseArea {
                        anchors.fill: parent; enabled: !dlg.editingName
                        onClicked: { dlg.tab = index; dlg.values = dlg.defaults(modelData); dlg.errors = ({}); kinds.positionViewAtIndex(index, ListView.Contain) }
                    }
                }
            }
            Column {
                id: formCol
                width: body.width - kinds.width - body.spacing; height: body.height; spacing: 14
                // The pages: fixed above the fields, so they stay put while a long page scrolls.
                Row {
                    id: pageStrip
                    objectName: "page-tabs"
                    visible: dlg.usable && dlg.pages().length > 0
                    height: visible ? 30 : 0; spacing: 0
                    Repeater {
                        model: dlg.pages()
                        delegate: Rectangle {
                            required property string modelData
                            objectName: "page-tab-" + modelData.toLowerCase()
                            readonly property bool on: dlg.page === modelData
                            // An error on a page you are not looking at shows on its tab.
                            readonly property bool bad: dlg.current() ? dlg.current().form.some(f => dlg.errors[f.key] && dlg.pageOf(f) === modelData) : false
                            width: Math.max(110, pageText.implicitWidth + 32); height: 30; color: "transparent"
                            Text { id: pageText; anchors.centerIn: parent; text: Kiki.T.has("location.page." + modelData.toLowerCase()) ? Kiki.T.tr("location.page." + modelData.toLowerCase()) : modelData; color: parent.bad ? Kiki.Theme.danger : (parent.on ? Kiki.Theme.fg : (pageHover.containsMouse ? Kiki.Theme.fgDim : Kiki.Theme.muted)); font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: parent.on }
                            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 2; color: parent.on ? Kiki.Theme.accent : Kiki.Theme.line }
                            MouseArea { id: pageHover; anchors.fill: parent; hoverEnabled: true; onClicked: dlg.page = modelData }
                        }
                    }
                }
                Column {
                    objectName: "location-unavailable"
                    visible: !dlg.usable
                    width: parent.width; spacing: 10; topPadding: 24
                    Text { text: dlg.current() ? Kiki.T.tr("location.notAvailable", { plugin: dlg.current().displayName }) : ""; color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: true }
                    Text { objectName: "location-unavailable-reason"; width: parent.width; wrapMode: Text.WordWrap; text: dlg.current() ? (dlg.current().unavailableReason || "") : ""; color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
                }
            Flickable {
                objectName: "location-form"
                visible: dlg.usable
                NaturalScroll { }
                width: parent.width; height: Math.max(0, parent.height - pageStrip.height - (pageStrip.visible ? parent.spacing : 0))
                clip: true; contentHeight: fields.height
                Column {
                    id: fields; width: parent.width; spacing: 14
                    Repeater {
                        model: dlg.formRows()
                        delegate: Item {
                            id: rowItem
                            required property var modelData
                            required property int index
                            /// The Name row carries the location's picture at its right.
                            readonly property bool hasImage: !!modelData.fields && modelData.fields[0].key === "name"
                            width: fields.width
                            height: modelData.tabs ? tabStrip.height : fieldRow.height
                            // The tabs: one way in at a time.
                            Row {
                                id: tabStrip
                                objectName: rowItem.modelData.tabs ? "auth-tabs" : ""      // every row has one; only one is it
                                visible: !!rowItem.modelData.tabs
                                spacing: 8; height: 28
                                Repeater {
                                    model: rowItem.modelData.tabs || []
                                    delegate: Rectangle {
                                        required property string modelData
                                        required property int index
                                        objectName: "auth-tab-" + modelData.toLowerCase()
                                        readonly property bool on: dlg.authGroup === modelData
                                        width: Math.max(104, tabText.implicitWidth + 32); height: 28; radius: 14
                                        color: on ? Kiki.Theme.accent : (tabHover.containsMouse ? Kiki.Theme.surface : Kiki.Theme.bgDark)
                                        border.width: 1; border.color: on ? Kiki.Theme.accent : Kiki.Theme.gutter
                                        Text { id: tabText; anchors.centerIn: parent; text: Kiki.T.has("location.group." + modelData.toLowerCase()) ? Kiki.T.tr("location.group." + modelData.toLowerCase()) : modelData; color: parent.on ? Kiki.Theme.bg : Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize; font.bold: parent.on }
                                        MouseArea { id: tabHover; anchors.fill: parent; hoverEnabled: true; onClicked: dlg.chooseGroup(modelData) }
                                    }
                                }
                            }
                            Row {
                                id: fieldRow
                                visible: !rowItem.modelData.tabs
                                width: parent.width; spacing: 12
                                Repeater {
                                    model: rowItem.modelData.fields || []
                                    delegate: FormField {
                                        required property var modelData
                                        required property int index
                                        readonly property int portWidth: 96
                                        // Alone it has the row; with a port beside it, the port takes its
                                        // five digits' worth and this takes the rest.
                                        width: rowItem.hasImage ? fieldRow.width - imageWell.width - fieldRow.spacing
                                             : rowItem.modelData.fields.length === 1 ? fieldRow.width
                                             : (modelData.kind === "port" ? portWidth : fieldRow.width - portWidth - fieldRow.spacing)
                                        field: modelData
                                        value: dlg.values[modelData.key] || ""
                                        choices: dlg.keyChoices[modelData.key] || []
                                        pickable: modelData.key === "localPath"
                                        onPick: dlg.chooseFolder(dlg.values[modelData.key] || "", path => { const nv = Object.assign({}, dlg.values); nv[modelData.key] = path; dlg.values = nv })
                                        error: dlg.errors[modelData.key] || ""
                                        onEdited: v => { const nv = Object.assign({}, dlg.values); nv[modelData.key] = v; dlg.values = nv }
                                        onBrowse: dlg.browseField(modelData.key)
                                    }
                                }
                                // Built like a field — label over box — so it lines up with Name
                                // whatever the font does.
                                Column {
                                    id: imageWell
                                    visible: rowItem.hasImage
                                    width: 44; spacing: 6
                                    Text { text: Kiki.T.tr("location.image"); color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11; font.letterSpacing: 0.6 }
                                    Rectangle {
                                        objectName: "location-image"
                                        readonly property bool has: wellPicture.shown
                                        width: parent.width; height: 32; radius: 2
                                        color: Kiki.Theme.bgDark; border.width: 1; border.color: wellHover.containsMouse ? Kiki.Theme.accent : Kiki.Theme.gutter
                                        Icon { anchors.centerIn: parent; name: "image"; visible: !parent.has; color: wellHover.containsMouse ? Kiki.Theme.accent : Kiki.Theme.fgDim }
                                        RoundedImage {
                                            id: wellPicture
                                            anchors.centerIn: parent; width: 26; height: 26; radius: 6; visible: parent.has
                                            source: dlg.image === "" ? "" : "file://" + encodeURI(dlg.image)
                                        }
                                        MouseArea {
                                            id: wellHover
                                            anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                                            onClicked: dlg.chooseImage(dlg.image, path => dlg.image = path)
                                        }
                                        Tip { visible: wellHover.containsMouse && !clearHover.containsMouse; text: dlg.image ? "Change the image" : "Choose an image for the sidebar" }
                                        // Back to the plain glyph.
                                        Rectangle {
                                            objectName: "location-image-clear"
                                            visible: dlg.image !== "" && (wellHover.containsMouse || clearHover.containsMouse)
                                            width: 16; height: 16; radius: 8; x: parent.width - 9; y: -7
                                            color: clearHover.containsMouse ? Kiki.Theme.danger : Kiki.Theme.surface; border.width: 1; border.color: Kiki.Theme.gutter
                                            Icon { anchors.centerIn: parent; name: "x"; size: 10; color: clearHover.containsMouse ? Kiki.Theme.bg : Kiki.Theme.fgDim }
                                            MouseArea { id: clearHover; anchors.fill: parent; hoverEnabled: true; onClicked: dlg.image = "" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            }
            }
            Column {
                id: footer
                width: parent.width; spacing: 8
                // One button, in the middle. There is no Cancel: the way out is the close box and
                // Esc, and a button beside Connect only offered somewhere to mis-click.
                // "Add" keeps it for later; "Add and Connect" opens it now. Both sign in first to
                // check the details — so neither is called "Connect", which said nothing about
                // the location being kept. Editing, they are "Save" and "Save and Connect".
                Row {
                    id: connectBtn
                    objectName: "location-buttons"
                    anchors.horizontalCenter: parent.horizontalCenter; spacing: 10
                    readonly property string verb: dlg.editingName ? Kiki.T.tr("location.save") : Kiki.T.tr("location.add")
                    Button { objectName: "location-add"; text: dlg.busy && !dlg.openAfter ? Kiki.T.tr("location.saving") : parent.verb; enabled: !dlg.busy && dlg.usable; onClicked: dlg.connect(undefined, false) }
                    Button { objectName: "location-add-connect"; text: dlg.busy && dlg.openAfter ? Kiki.T.tr("location.connecting") : Kiki.T.tr("location.andConnect", { verb: parent.verb }); primary: true; enabled: !dlg.busy && dlg.usable; onClicked: dlg.connect(undefined, true) }
                }
                // What is happening, or went wrong, under it — the full width to say it in.
                Text {
                    objectName: "location-status"
                    visible: text !== ""
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap; maximumLineCount: 3; elide: Text.ElideRight
                    text: dlg.status; color: dlg.status && !dlg.busy ? Kiki.Theme.danger : Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
                }
            }
        }
    }
    // Verify the server. Shown over the form, because it is a decision about the connection the
    // form just made, and answering it is what saves the location.
    // Dim the form behind the question: it is the only thing to answer, and answering it is what
    // saves the location.
    Rectangle {
        visible: dlg.verifyFingerprint !== ""
        anchors.fill: parent; color: Qt.rgba(0, 0, 0, 0.45); z: 3
        MouseArea { anchors.fill: parent }
    }
    Rectangle {
        objectName: "verify-host"
        visible: dlg.verifyFingerprint !== ""
        anchors.centerIn: parent
        width: Math.min(520, dlg.width - 32); height: col.height + 48
        radius: 2; color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.yellow
        z: 4
        MouseArea { anchors.fill: parent }        // the form behind must not take clicks
        Column {
            id: col
            y: 24; x: 24; width: parent.width - 48; spacing: 14
            Row {
                spacing: 10
                Icon { name: "warn"; size: 18; color: Kiki.Theme.yellow; anchors.verticalCenter: parent.verticalCenter }
                Text { text: Kiki.T.tr("location.verifyTitle"); color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 15; font.bold: true; anchors.verticalCenter: parent.verticalCenter }
            }
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                text: (dlg.verifyHost ? Kiki.T.tr("location.identified", { host: dlg.verifyHost }) : Kiki.T.tr("location.serverIdentified")) +
                      " with a key kiki has not seen before. Check it against the fingerprint the server's own administrator published — on the server, `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub` prints it."
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
            Rectangle {
                width: parent.width; height: fp.height + 20; radius: 2
                color: Kiki.Theme.bgDark; border.width: 1; border.color: Kiki.Theme.line
                Text {
                    id: fp
                    objectName: "verify-fingerprint"
                    x: 10; y: 10; width: parent.width - 20; wrapMode: Text.WrapAnywhere
                    text: dlg.verifyFingerprint
                    color: Kiki.Theme.fg; font.family: Kiki.Theme.mono; font.pixelSize: 12
                }
            }
            Text {
                width: parent.width; wrapMode: Text.WordWrap
                text: Kiki.T.tr("location.verifyNote")
                color: Kiki.Theme.muted; font.family: Kiki.Theme.mono; font.pixelSize: 11
            }
            Row {
                anchors.right: parent.right; spacing: 8
                Button { objectName: "verify-cancel"; text: Kiki.T.tr("common.cancel"); onClicked: { if (dlg.verifyOnly) dlg._leaveVerify(""); else { dlg.verifyFingerprint = ""; dlg.status = Kiki.T.tr("location.notAccepted") } } }
                Button { objectName: "verify-trust"; text: dlg.busy ? Kiki.T.tr("location.saving") : Kiki.T.tr("location.trustSave"); primary: true; enabled: !dlg.busy; onClicked: dlg.connect(dlg.verifyFingerprint) }
            }
        }
        Keys.onEscapePressed: if (dlg.verifyOnly) dlg._leaveVerify(""); else dlg.verifyFingerprint = ""
    }
    // Verify-only, before the server has answered: there is no form to show "Connecting…" in.
    Rectangle {
        objectName: "verify-waiting"
        visible: dlg.verifyOnly && dlg.verifyFingerprint === ""
        anchors.centerIn: parent; z: 4
        width: waitText.implicitWidth + 48; height: 56; radius: 2
        color: Kiki.Theme.bg; border.width: 2; border.color: Kiki.Theme.yellow
        Text { id: waitText; anchors.centerIn: parent; text: Kiki.T.tr("location.asking", { host: dlg.values.host || Kiki.T.tr("location.theServer") }); color: Kiki.Theme.fgDim; font.family: Kiki.Theme.mono; font.pixelSize: Kiki.Theme.fontSize }
    }

    focus: visible
    activeFocusOnTab: true
    onVisibleChanged: if (visible) { page = "Connection"; forceActiveFocus(); _keysDefaulted = ({}); loadKeyChoices() }
    // Tab walks on from the kind strip into the fields and then the buttons.
    // Enter is the primary button: Add and Connect. (Trusting a key keeps whichever was asked for.)
    Keys.onReturnPressed: if (dlg.verifyFingerprint) dlg.connect(dlg.verifyFingerprint); else dlg.connect(undefined, true)
    Keys.onEnterPressed: if (dlg.verifyFingerprint) dlg.connect(dlg.verifyFingerprint); else dlg.connect(undefined, true)
    // A text field takes Left/Right first, so these only fire when the strip has the keyboard.
    Keys.onLeftPressed: dlg.selectKind(dlg.tab - 1)
    Keys.onRightPressed: dlg.selectKind(dlg.tab + 1)
    Keys.onEscapePressed: visible = false
    ContextMenu { id: browseMenu; parent: dlg }
}
