pragma Singleton
import QtQuick

Singleton {
    function bytes(n) {
        if (n === undefined || n === null) return ""
        if (n < 1024) return n + " B"
        const u = ["KB", "MB", "GB", "TB"]; let v = n / 1024, i = 0
        while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
        return (v < 10 ? v.toFixed(1) : Math.round(v)) + " " + u[i]
    }
    function date(ms) {
        if (!ms) return ""
        return new Date(ms).toLocaleString(Qt.locale(), "d MMM yyyy HH:mm")
    }
    function gitBadge(g) { if (!g) return ""; return { modified: "M", added: "A", deleted: "D", renamed: "R", conflicted: "!", untracked: "?", ignored: "", clean: "" }[g.state] || "" }
    function gitColor(g) {
        const t = Qt.resolvedUrl("") // no-op to keep this a plain function
        if (!g) return "#565f89"
        return { modified: "#e0af68", added: "#9ece6a", deleted: "#f7768e", renamed: "#e0af68", conflicted: "#f7768e", untracked: "#7f9e6a", ignored: "#565f89", clean: "#565f89" }[g.state] || "#565f89"
    }
    function kindLabel(kind) {
        return { folder: "Folder", image: "Image", video: "Video", audio: "Audio", code: "Code", text: "Text", document: "Document", pdf: "PDF document", archive: "Archive", link: "Link", file: "File", other: "Other" }[kind] || "File"
    }
    // ~-shortened display of a URI, as the daemon's Uri::display does.
    function display(uri, home) {
        if (!uri) return ""
        if (uri.startsWith("file://")) {
            let p = decodeURIComponent(uri.slice(7))
            if (home && p === home) return "~"
            if (home && p.startsWith(home + "/")) return "~/" + p.slice(home.length + 1)
            return p
        }
        const i = uri.indexOf("://")
        return decodeURIComponent(uri.slice(i + 3))
    }
    function crumbs(uri, home) {
        const d = display(uri, home)
        const parts = d.split("/").filter(s => s.length)
        if (d.startsWith("/")) parts.unshift("/")
        return parts
    }
}
