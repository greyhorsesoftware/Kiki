import QtQuick
import Quickshell
import Quickshell.Io
import "." as Kiki

// The window's IPC surface — `qs ipc call shell <function> …` — split out of Shell.qml
// (docs/0.5.0/04-shell-split.md): what the harness, the `kiki` command and `kiki-dbus` reach the
// window with. Nothing here decides anything: every function calls the function the click or
// the key calls on `win`, so there is no second path to drift. The handler keeps its target,
// "shell", and its functions keep their names; a flow that drove the window before the split
// drives it the same after.
Item {
    id: ipc
    visible: false
    required property var win

        IpcHandler {
            target: "shell"
            /// Open `uri` in the focused pane, measuring what the opening cost from this side of the
            /// socket; `openStats` says `running` until the first screenful is whole. The open_perf
            /// flow (docs/0.5.0/10-faster-listings.md); nothing here depends on it.
            function openProbe(uri: string): void { win.openProbe.start(win.pane, uri) }
            function openStats(): string { return JSON.stringify(win.openProbe.result) }
            function open(uri: string): void { win.pane.open(uri) }
            /// What `kiki-dbus` puts to this window: another application's "Show in file manager",
            /// or the file chooser the portal asked for (docs/0.3.0/01-daemon-on-demand.md,
            /// decision 4). The listener is started by the bus and has no connection to the daemon;
            /// it reaches the window here, as the `kiki` command does.
            ///
            /// A chooser cannot be answered from this call — the person has not chosen yet, and QML
            /// answers at once or not at all — so it opens the dialog, returns a token, and the
            /// listener asks `ChooserPoll` for the answer until it has one.
            function dbus(kind: string, payload: string): string {
                let r = {}
                try { r = payload ? JSON.parse(payload) : {} } catch (e) { return JSON.stringify({ error: "bad payload" }) }
                switch (kind) {
                case "ShowItems": win.showItems(r); return "{}"
                case "ShowFolders": win.showItems(Object.assign({}, r, { folders: true })); return "{}"
                case "ShowChooser": win.startChooser(r); return JSON.stringify({ token: r.token || "" })
                case "ChooserPoll": return JSON.stringify(win.chooserAnswer(r.token || ""))
                }
                return JSON.stringify({ error: "unknown " + kind })
            }
            /// What the `kiki` launcher calls on a running instance: open it, and come to the front.
            function present(uri: string): void { win.present(uri) }
            function enter(): void { win.enterSelected() }
            /// Mirrors the gallery's keys, fallback included, so the harness can drive them.
            function gallery(action: string): void {
                const g = win.galleryPane(); if (!g) return
                if (action === "prev") { if (!g.step(-1)) win.pane.up() }
                else if (action === "next") g.step(1)
                else if (action === "open") g.activateKey()
                else if (action === "play") g.togglePlay()
            }
            /// What the gallery cost: how many pictures it has decoded and how long they took. The
            /// perf flow reads this; nothing in the window depends on it.
            function galleryStats(): string {
                const g = win.galleryPane()
                return JSON.stringify(g ? g.stats() : {})
            }
            /// Scroll the focused pane's view from top to bottom in `ms`, measuring; `scrollStats`
            /// says `running` until it is over. The scroll_perf flow; nothing here depends on it.
            function scrollRun(ms: string): void {
                const v = win.currentView(), s = v && v.scroller ? v.scroller() : null
                win.scrollProbe.start(s ? s.view : null, s ? s.cache : null, parseInt(ms) || 4000)
            }
            function scrollStats(): string { return JSON.stringify(win.scrollProbe.result) }
            /// A drop, without a pointer: `uris` is one or more URIs separated by newlines — the very
            /// shape of a `text/uri-list`, and not JSON, because Quickshell's IPC eats square brackets
            /// out of an argument. `dest` is the folder it lands in — `trash:///` is the sidebar's
            /// Trash — and `modifiers` any of ctrl/shift/alt. It goes through the same
            /// `Pane.dropInto` a real drag does, with an event shaped as Qt shapes one (`fakeDrop`: the
            /// keys folded into `proposedAction`), so a flow drives the code a hand drives, less the
            /// press and the pointer. Answers what the drop decided.
            function drop(uris: string, dest: string, modifiers: string): string { return win.fakeDrop(win.pane, uris, dest, modifiers) }
            /// The same, into a pane named `left` or `right` rather than the focused one — the pane a
            /// drop lands in takes the focus, and this is how a flow sees that happen. Naming a pane
            /// is also how a flow drops on the trash VIEW's folder rather than on the sidebar's
            /// Trash: the same `trash:///`, two different targets, and this one is refused.
            function dropOn(side: string, uris: string, dest: string, modifiers: string): string {
                return win.fakeDrop(side === "right" ? win.right : win.left, uris, dest, modifiers, true)
            }
            /// The yes/no question, for scripts and tests: `yes` or `no` answers it, anything else
            /// leaves it up. Either way, answers what it was asking.
            function question(answer: string): string {
                const was = { open: win.confirm.visible, title: win.confirm.title, message: win.confirm.message, label: win.confirm.confirmLabel, danger: win.confirm.danger }
                if (win.confirm.visible && (answer === "yes" || answer === "no")) win.confirm.answer(answer === "yes")
                return JSON.stringify(was)
            }
            /// A rebindable action by its keymap id (`trash`, `deleteForever`, …): what its key does,
            /// for a flow that has no keyboard.
            function action(id: string): void { win.runAction(id) }
            function back(): void { win.pane.back() }
            function forward(): void { win.pane.forward() }
            function setView(v: string): void { win.pane.view = v }
            function search(text: string): void { if (text) win.openFilter(); const bar = win.pane === win.right ? win.rightFilter : win.leftFilter; bar.text = text; win.applyFilter(text) }
            function searchEverywhere(text: string): void { if (win.searchOverlay.visible && !text) win.searchOverlay.close(); else win.openSearch(text) }
            /// A selection of several, by name, comma-separated; the last named is the current row.
            function selectMany(names: string): void {
                const want = names.split(","), at = []
                for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && want.indexOf(r.name) >= 0) at.push(i) }
                if (at.length) win.pane.selection.setMany(at, at[at.length - 1])
            }
            function select(name: string): void {
                for (let i = 0; i < win.pane.listing.count; i++) {
                    const r = win.pane.listing.row(i)
                    if (r && r.name === name) { win.pane.selection.set(i); return }
                }
                // Past the rows the window happens to hold — it keeps a few hundred either side of
                // the viewport, not the whole folder — only the daemon knows where a name sits.
                if (win.pane.listing.lid) win.seekName(name)
            }
            function selection(): string { return JSON.stringify(win.selectedUris()) }
            function uri(pane: string): string { return win.pane.uri }
            function split(on: string): void { if (on === "on") win.enterMirror(); else win.leaveMirror() }
            function project(action: string, uri: string): void { if (action === "enter") win.enterProject(uri || win.pane.uri); else win.leaveProject() }
            function projectState(): string { return JSON.stringify({ root: win.projectRoot, active: win.projectMode, width: win.width }) }
            function edit(uri: string, line: string): void { win.editAt(uri, parseInt(line) || 1) }
            function reveal(uri: string): void { if (win.projectMode) win.projectTree.reveal(uri); else { const p = uri.replace(/\/[^/]*$/, ""); win.pane.open(p); const name = decodeURIComponent(uri.split("/").pop()); Qt.callLater(() => { for (let i = 0; i < win.pane.listing.count; i++) { const r = win.pane.listing.row(i); if (r && r.name === name) { win.pane.selection.set(i); break } } }) } }
            function saved(uri: string): void { win.pane.listing.refresh() }
            /// `share` alone opens the menu; `share <plugin>` sends to it, as clicking it would.
            function share(plugin: string, target: string): void {
                if (!plugin) { win.shareMenu(); return }
                const p = win.sharePlugins.find(x => x.id === plugin); if (!p) return
                const uris = win.selectedUris(); if (!uris.length) return
                if (p.targets === "none" && !target) win.shareNow(p, null, uris)
                else win.shareTargets(p, uris)
            }
            function settings(action: string, page: string): void { if (action === "open") win.settingsWin.open(page || "general"); else { win.settingsWin.close(); win.keys.forceActiveFocus() } }
            /// Quick Look, for the harness: `open` is Space on the selected file, `close` is Space
            /// again (or Esc in the window), `toggle` is either; `step <n>` is j or k inside the
            /// window; `key <name>` is a key pressed in it — escape, space, j, k. Answers what the
            /// window shows, as `state` carries it.
            function quickLook(action: string): string {
                const a = (action || "").trim().split(/\s+/)
                if (a[0] === "open") win.openQuickLook()
                else if (a[0] === "close") win.quickLookWin.close()
                else if (a[0] === "toggle") win.toggleQuickLook()
                else if (a[0] === "step") win.quickLookWin.step(parseInt(a[1]) || 1)
                else if (a[0] === "key") { const k = { escape: Qt.Key_Escape, space: Qt.Key_Space, j: Qt.Key_J, k: Qt.Key_K, down: Qt.Key_Down, up: Qt.Key_Up, left: Qt.Key_Left, right: Qt.Key_Right }[a[1]]; if (k !== undefined) win.quickLookWin.handleKey(k, 0) }
                return JSON.stringify(win.quickLookState())
            }
            /// The mirror workspace (plan 08's four screens) without a pointer. One word and its
            /// arguments, space-separated, because Quickshell's IPC hands a function strings:
            ///   `open` / `download`   start a run from the local or the remote side
            ///   `set <option> <value>`  direction, detector, deletes, filters, window, windowValue,
            ///                           windowUnit, `offset auto` | `offset <hours>` — what the
            ///                           Configure screen's controls set
            ///   `preflight`           the Preflight button: scan, then Review
            ///   `tab <all|new|changed|equal|delete>`  a Review tab
            ///   `check <row> <on|off>`  a click on a row's box
            ///   `report`              what "Save report…" saves, into `report` below
            ///   `run`                 the Mirror button; `confirm yes|no` answers the large-delete
            ///                         question it may ask
            ///   `cancel`              the Cancel button: the compare on Preflight, the run on Running
            ///   `escape`              what the Escape key does — Cancel on Preflight, nothing else
            ///   `back` / `close`      Back, and the button that leaves
            ///   `rules`               read the filter rules, into `rules` below
            ///   `rules-edit`          the Edit rules… button
            ///   `rules-set <lines>`   the rules dialog's Done: one `<kind> <value>` per LINE, because
            ///                         IPC eats the quotes out of an argument; nothing at all is "no
            ///                         rules", which is not the same as the defaults
            ///   `rules-defaults`      Restore defaults, and Done
            ///   `rules-cancel`        the rules dialog's Cancel
            /// Each of them calls the function the click calls, so there is no second path to drift.
            /// Answers what the workspace is showing: screen, options, plan counts and rows, the
            /// question if it is up, the Done summary and the filter rules.
            function mirror(action: string): string {
                const a = (action || "").trim().split(/\s+/)
                switch (a[0]) {
                case "open": win.startMirror(true); break
                case "download": win.startMirror(false); break
                case "close": win.mirrorWs.leave(); break
                case "set": win.mirrorWs.setOption(a[1], a[2]); break
                case "preflight": win.mirrorWs.preflight(); break
                case "tab": win.mirrorWs.setTab(a[1] || "all"); break
                case "check": win.mirrorWs.toggleRow(parseInt(a[1]) || 0, a[2] !== "off"); break
                case "report": win.mirrorWs.fetchReport(); break
                // The button itself: the chooser comes up; `save <uri>` is what choosing that file does.
                case "saveReport": win.mirrorWs.saveReport(); break
                case "save": if (win.chooserUp) win.finishChooser([a[1]]); break
                case "run": win.mirrorWs.mirror(false); break
                case "confirm": win.mirrorWs.answerLargeDelete(a[1] === "yes"); break
                case "cancel": win.mirrorWs.stop(); break
                case "escape": win.mirrorWs.escapeKey(); break
                case "back": win.mirrorWs.back(); break
                // The filter rules: `rules` reads them, `rules-edit` is the Edit rules… button, and
                // `rules-set` is the dialog's Done. The rules come one per line rather than as JSON,
                // because IPC strips the quotes out of an argument — the same reason `drop` takes its
                // URIs newline-separated.
                case "rules": win.mirrorWs.loadRules(); break
                case "rules-edit": win.mirrorWs.editRules(); break
                case "rules-set": win.mirrorWs.setRules(action.replace(/^[ \t]*rules-set[ \t]*/, "")); break
                case "rules-defaults": win.mirrorWs.restoreRules(); break
                case "rules-cancel": win.mirrorWs.cancelRules(); break
                }
                return JSON.stringify(win.mirrorWs.info())
            }
            function mirrorScreen(): string { return win.mirrorOpen ? win.mirrorWs.screen : "" }
            function focusPane(side: string): void { win.focusPane(side === "right" ? win.right : win.left) }
            function transfer(kind: string): void { win.transfer(kind === "move") }
            function openLocation(name: string): void { const l = win.locations.find(x => x.name === name); if (l) win.openLocation(l) }
            /// The sidebar menu's Disconnect, by name; `locationDots` is which locations wear the green dot.
            function disconnectLocation(name: string): void { win.disconnectLocation(name) }
            function locationDots(): string { return JSON.stringify(win.locations.filter(l => l.connected).map(l => l.name)) }
            function state(): string {
                return JSON.stringify({ uri: win.pane.uri, view: win.pane.view, count: win.pane.listing.count, done: win.pane.listing.done, error: win.pane.listing.error, selection: win.selectedUris(), inspector: win.inspector, sidebar: win.sidebarShown, keyFocus: win.keys.activeFocus, filterOpen: win.filterOpen, searchOpen: win.searchOverlay.visible, settingsVisible: win.settingsWin.visible, menuVisible: win.menu.visible, clipboard: win.clipboard.uris, clipboardCut: win.clipboard.cut === true, renaming: win.renamingRow(),
                    daemon: { ready: Kiki.Daemon.ready, connected: Kiki.Daemon.connected },
                    dialogs: { confirm: win.confirm.visible, compress: win.compressDialog.visible, location: win.locationDialog.visible, integration: win.integrationDialog.visible, portal: win.chooserUp, share: win.shareSheet.visible }, split: win.split, infoPopover: win.infoPopover.visible, infoRows: win.inspectedRows.length, filter: win.pane.filterText, filterColumn: (win.pane.view === "columns" && win.currentView()) ? win.currentView().focusCol : -1, sort: [win.pane.sortRole, win.pane.sortOrder], toast: win.toast,
                    listColumns: win.listColumnWidths(), quickLook: win.quickLookState() })
            }
            /// Side by side, for scripts and tests: `toggle`; `drag <px>` is what dragging the line
            /// between the panes to that x does, `end` lets go, `reset` is the double click.
            function sideBySide(action: string): string {
                if (action === "toggle") win.toggleMirrorView(true)
                else if (action.indexOf("drag ") === 0) win.dragDivider(win.paneRow.width, parseInt(action.slice(5)) || 0)
                else if (action === "end") win.endDividerDrag()
                else if (action === "reset") win.resetDivider()
                return JSON.stringify({ split: win.split, total: win.paneRow.width, left: win.split ? win.leftPaneWidth(win.paneRow.width) : win.paneRow.width,
                    ratio: win.sideRatio, dragging: win.sideRatioLive > 0, min: win.sideMin,
                    leftView: win.left.view, rightView: win.right.view, remembering: win.left.rememberViews,
                    titlePathShown: win.toolbar.pathShown, leftPath: win.leftHeader.breadcrumb.visible ? win.leftHeader.breadcrumb.uri : "", rightPath: win.rightHeader.breadcrumb.visible ? win.rightHeader.breadcrumb.uri : "", focused: win.pane === win.right ? "right" : "left", leftUri: win.left.uri, rightUri: win.right.uri })
            }
            function viewMenu(): void { if (win.menu.visible) win.menu.close(); else win.viewMenu() }
            /// What the open menu says, for scripts and tests: each row's label, tick and whether it is live.
            function menuItems(): string { return JSON.stringify(win.menu.visible ? win.menu.items.map(i => ({ label: i.label, checked: i.checked === true, enabled: !win.menu.off(i) })) : []) }
            function pathMenu(): void { if (win.menu.visible) win.menu.close(); else win.pathMenu() }
            function toggleSearch(): void { win.toggleSearch() }
            function inspector(on: string): void { win.inspectorRequested = on === "" ? !win.inspectorRequested : on === "on" }
            function openWith(): void { if (win.menu.visible) win.menu.close(); else win.openWithMenu() }
            function columns(action: string): string {
                const c = win.columnsPane(); if (!c) return ""
                if (action === "down") c.moveKey(1); else if (action === "up") c.moveKey(-1)
                else if (action === "left") c.focusLeft(); else if (action === "right") c.focusRight()
                else if (action === "open") c.activateKey()
                // `info <px>` is what a drag on the info column's edge does, for scripts and tests.
                else if (action.indexOf("info ") === 0) c.inspectorW = parseInt(action.slice(5)) || 0
                return JSON.stringify({ focusCol: c.focusCol, count: c.columns.length, selected: c.columns.map(x => x.selected),
                                        inspected: c.inspectedUri, scrollX: Math.round(c.scrollX), width: Math.round(c.stripWidth),
                                        columnWidth: c.columnWidth, infoWidth: c.inspectedUri !== "" ? c.inspectorWidth : 0 })
            }
            /// A list column's width, for scripts and tests, following `columns info <px>`:
            /// `listColumn <role> <px>` is what dragging that column's edge to that width does — the
            /// same function, clamped the same way — `listColumn <role> reset` is the double click on
            /// it, and `listColumn` alone only reads. Answers the widths the list is drawing, name
            /// included; `shell state` carries them too.
            function listColumn(role: string, px: string): string {
                const l = win.listPane()
                if (l && role) {
                    if (px === "reset") l.resetColumnWidth(role)
                    else if (px !== "") { l.setColumnWidth(role, parseInt(px) || 0); l.endColumnResize() }
                }
                return JSON.stringify(win.listColumnWidths())
            }
            function sidebar(on: string): void { win.sidebarShown = on === "" ? !win.sidebarShown : on === "on" }
            function undo(): void { Kiki.Jobs.undo() }
            function redo(): void { Kiki.Jobs.redo() }
            function activity(): string { return JSON.stringify(Kiki.Jobs.list) }
            /// The orb and its popup, for the harness: "open" | "close" | "toggle" | "clear", and what is showing.
            function activityView(action: string): string {
                if (action === "open") win.activity.open(); else if (action === "close") win.activity.close(); else if (action === "toggle") win.activity.toggle(); else if (action === "clear") Kiki.Jobs.clear()
                return JSON.stringify({ open: win.activity.visible, orb: Kiki.Jobs.orbState(), tip: Kiki.Jobs.orbTip(), entries: Kiki.Jobs.shown().map(j => ({ id: j.id, headline: Kiki.Jobs.headline(j), state: j.state, line: Kiki.Jobs.live(j) ? Kiki.Jobs.statusLine(j) : Kiki.Jobs.completion(j) })) })
            }
            function contextMenu(action: string): void { const it = win.contextItemsNow().find(i => i.id === action || i.label === action); if (it && it.enabled !== false && it.action) it.action() }
            function addLocation(): void { win.locationDialog.open(null) }
            /// The Add-location form, for scripts and tests: pick a kind by scheme, a page or a
            /// credentials tab by name.
            function locationForm(what: string, name: string): string {
                if (what === "kind") win.locationDialog.selectKind(win.locationDialog.plugins.findIndex(p => p.scheme === name))
                else if (what === "page") win.locationDialog.page = name
                else if (what === "auth") win.locationDialog.chooseGroup(name)
                return JSON.stringify({ kind: win.locationDialog.current() ? win.locationDialog.current().scheme : "", page: win.locationDialog.page, auth: win.locationDialog.authGroup, pages: win.locationDialog.pages(), groups: win.locationDialog.groups() })
            }
            function about(): void { if (win.aboutDlg.visible) win.aboutDlg.close(); else win.aboutDlg.open() }
            /// The palette in force, for scripts and for checking a theme change landed.
            function theme(): string {
                return JSON.stringify({ name: Kiki.Theme.name, icons: Kiki.Theme.iconTheme,
                                        folderIcon: Quickshell.iconPath("folder", true), bg: String(Kiki.Theme.bg),
                                        fg: String(Kiki.Theme.fg), accent: String(Kiki.Theme.accent), surface: String(Kiki.Theme.surface) })
            }
            function keymap(): void { if (win.keysWin.visible) win.keysWin.close(); else win.keysWin.open() }
            /// Where an element is, in window coordinates, for a test that drives the pointer: the
            /// rectangle of the first item with this objectName, or an empty object when nothing has
            /// it. Names follow plan 28: row-N, tile-N, column-N, menu-LABEL, perm-WHO-BIT …
            function geometry(name: string): string {
                const it = win.findByName(win.contentItem, name)
                return it ? win.rectOf(it) : "{}"
            }
            /// Where row `i` of the current view is, in window coordinates. Rows move as a folder
            /// loads, so this asks the view for the delegate rather than searching by name.
            function rowGeometry(index: string): string {
                const pane = win.currentView()
                const it = pane && pane.rowItem ? pane.rowItem(parseInt(index)) : null
                return it ? win.rectOf(it) : "{}"
            }
            /// Close whatever is open — editor, menu, overlay — and hand the keymap its focus back.
            /// What Escape does, for a script that cannot be sure what the last step left behind.
            function dismiss(): void {
                win.pane.renamingIndex = -1
                const cols = win.columnsPane(); if (cols) cols.cancelRename()
                win.menu.close()
                if (win.searchOverlay.visible) win.searchOverlay.close()
                win.filterOpen = false
                win.pane.selection.clear()
                win.keys.forceActiveFocus()
            }
            function windowState(pane: string): string { const l = win.pane.listing; return JSON.stringify({ count: l.count, viewport: [l.viewportFirst, l.viewportCount], held: Object.keys(l._rows).length }) }
            function timestamps(): string { return JSON.stringify({ now: Date.now() }) }
        }
}
