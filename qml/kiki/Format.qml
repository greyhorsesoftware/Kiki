pragma Singleton
import QtQuick

QtObject {
    function bytes(n) {
        if (n === undefined || n === null) return ""
        if (n < 1024) return n + " B"
        const u = ["KB", "MB", "GB", "TB"]; let v = n / 1024, i = 0
        while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
        return (v < 10 ? v.toFixed(1) : Math.round(v)) + " " + u[i]
    }
    // "just now", "5 min ago", "3 h ago", "2 days ago", "3 weeks ago", "5 months ago", "2 years ago"
    function relative(ms) {
        if (!ms) return "—"
        const s = Math.max(0, (Date.now() - ms) / 1000)
        if (s < 45) return "just now"
        if (s < 3600) return Math.round(s / 60) + " min ago"
        if (s < 86400) return Math.round(s / 3600) + " h ago"
        const d = Math.round(s / 86400)
        if (d < 14) return d + (d === 1 ? " day ago" : " days ago")
        if (d < 60) return Math.round(d / 7) + " weeks ago"
        if (d < 365) return Math.round(d / 30) + " months ago"
        const y = Math.round(d / 365)
        return y + (y === 1 ? " year ago" : " years ago")
    }
    // Heat for an access time: the accent colour at full strength for the last hour, fading on a
    // log scale to nothing at about a year. Returns a colour with alpha, meant as a cell background.
    function heat(ms, accent) {
        if (!ms) return Qt.rgba(0, 0, 0, 0)
        const hours = Math.max(1, (Date.now() - ms) / 3600000)
        const t = Math.max(0, 1 - Math.log(hours) / Math.log(24 * 365))   // 1 = now, 0 = a year
        return Qt.rgba(accent.r, accent.g, accent.b, 0.05 + 0.45 * t * t)
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
