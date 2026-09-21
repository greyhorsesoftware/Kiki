pragma Singleton
import QtQuick
import "." as Kiki

QtObject {
    function bytes(n) {
        if (n === undefined || n === null) return ""
        if (n < 1024) return n + " B"
        const u = ["KB", "MB", "GB", "TB"]; let v = n / 1024, i = 0
        while (v >= 1024 && i < u.length - 1) { v /= 1024; i++ }
        return (v < 100 ? v.toFixed(1) : Math.round(v)) + " " + u[i]
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
        // 1.5 rather than 2: plan 22 fixes three points — 0.5 at an hour, about 0.28 at a day,
        // 0.05 at a year — and squaring undershoots the middle one (0.24).
        return Qt.rgba(accent.r, accent.g, accent.b, 0.05 + 0.45 * Math.pow(t, 1.5))
    }
    // Modified column, friendly form: "just now", "12 min ago", "3 h ago", "yesterday 14:02",
    // "Tuesday 14:02" (this week), "12 Sep 14:02" (this year), "12 Sep 2024" (older).
    function friendlyDate(ms) {
        if (!ms) return ""
        const d = new Date(ms), now = new Date()
        const s = (now.getTime() - ms) / 1000
        if (s >= 0 && s < 45) return "just now"
        if (s >= 0 && s < 3600) return Math.round(s / 60) + " min ago"
        const sameDay = d.toDateString() === now.toDateString()
        if (s >= 0 && s < 86400 && sameDay) return Math.round(s / 3600) + " h ago"
        const y = new Date(now); y.setDate(now.getDate() - 1)
        const hm = d.toLocaleTimeString(Qt.locale(), "HH:mm")
        if (d.toDateString() === y.toDateString()) return "yesterday " + hm
        if (s > 0 && s < 6 * 86400) return d.toLocaleDateString(Qt.locale(), "dddd") + " " + hm
        if (d.getFullYear() === now.getFullYear()) return d.toLocaleString(Qt.locale(), "d MMM HH:mm")
        return d.toLocaleString(Qt.locale(), "d MMM yyyy")
    }
    function modified(ms) { return Kiki.Settings.view.relativeDates === false ? date(ms) : friendlyDate(ms) }
    function date(ms) {
        if (!ms) return ""
        return new Date(ms).toLocaleString(Qt.locale(), "d MMM yyyy HH:mm")
    }
    /// What a row shows of git, after the settings have had their say: nothing for a folder when
    /// `[git] folders = "off"`, nothing for a clean or an ignored row (an ignored one is dimmed,
    /// not marked). Every view asks this, so they cannot disagree.
    function gitMark(row) {
        const g = row ? row.git : null
        if (!g || g.state === "clean" || g.state === "ignored") return null
        if (row.isDir && Kiki.Settings.git.folders === "off") return null
        // A folder that is a repository of its own wears the capsule instead, in the same place:
        // it says which branch as well as how it stands, and two marks would be one too many.
        if (g.root) return null
        return g
    }
    /// A folder that is itself a repository — the folder above it may be in no repository at all,
    /// so this is the only thing that can speak for it (plan 15). The capsule goes where the
    /// letter badge goes and is coloured by `gitColor` like everything else. It is a folder mark,
    /// so `[git] folders = "off"` takes it away with the rest of them.
    function gitCapsule(row) {
        const g = row ? row.git : null
        if (!g || !g.root || !g.branch) return null
        if (Kiki.Settings.git.folders === "off") return null
        return g
    }
    /// What an icon tile's dot is coloured by: the row's own mark, or — a tile has no room for a
    /// branch, and gets no new elements — a repository root's aggregate on its own.
    function gitDotMark(row) { return gitMark(row) || gitCapsule(row) }
    /// Ignored rows are dimmed unless `[git] showIgnored = "normal"`. ("hide" never gets here: the
    /// daemon leaves those rows out of the listing.)
    function gitDimmed(row) {
        return !!row && !!row.git && row.git.state === "ignored" && Kiki.Settings.git.showIgnored !== "normal"
    }
    /// A size as a transfer shows it (plan 32): bytes under 1 KB, whole KB, MB to one decimal,
    /// GB to two — coarse while small, finer as a rate or a remainder starts to matter.
    function transferSize(n) {
        n = Math.max(0, n || 0)
        if (n < 1024) return n + " B"
        if (n < 1024 * 1024) return Math.round(n / 1024) + " KB"
        if (n < 1024 * 1024 * 1024) return (n / (1024 * 1024)).toFixed(1) + " MB"
        return (n / (1024 * 1024 * 1024)).toFixed(2) + " GB"
    }
    /// An error as a person wants it: a plugin's or a library's message often arrives wrapped in
    /// the names of the things that passed it along — "Io: russh::Error: Network is unreachable" —
    /// and they nest. Every leading `Name:` that looks like a type or a path of types goes.
    function cleanError(msg) {
        let m = (msg || "").trim()
        for (;;) {
            const hit = m.match(/^(?:[A-Za-z_][\w]*(?:(?:::|\.)[A-Za-z_][\w]*)+|[A-Z][A-Za-z]*(?:Error|Exception)|Io|Invalid)\s*:\s+([\s\S]+)$/)
            if (!hit) break
            m = hit[1].trim()
        }
        // The daemon's bare codes, as words: "report.pdf: NotFound" reads like a stack trace.
        const words = { NotFound: "not found", Denied: "permission denied", Exists: "already exists", NotEmpty: "the folder is not empty", Unsupported: "not supported here" }
        m = m.replace(/(^|: )(NotFound|Denied|Exists|NotEmpty|Unsupported)$/, (all, lead, code) => lead + words[code])
        return m || "Failed"
    }
    /// A length of time as a player shows it: 0:07, 3:40, 1:02:05.
    function clock(ms) {
        const t = Math.max(0, Math.floor((ms || 0) / 1000))
        const h = Math.floor(t / 3600), m = Math.floor((t % 3600) / 60), s = t % 60
        const two = n => (n < 10 ? "0" : "") + n
        return h > 0 ? h + ":" + two(m) + ":" + two(s) : m + ":" + two(s)
    }
    function gitBadge(g) { if (!g) return ""; return { modified: "M", added: "A", deleted: "D", renamed: "R", conflicted: "!", untracked: "?", ignored: "", clean: "" }[g.state] || "" }
    /// Plan 15's colours, taken from the theme rather than written down: a badge is part of the
    /// window and changes with it, like everything else drawn beside it. Gone and conflicted are
    /// `danger` rather than `red` — a meaning, not a palette slot, so a theme whose red is green
    /// does not paint a lost file in it. Untracked is the theme's green most of the way to muted:
    /// there, but not yours yet.
    function gitColor(g) {
        if (!g) return Kiki.Theme.muted
        switch (g.state) {
        case "modified": case "renamed": return Kiki.Theme.changed
        case "added": return Kiki.Theme.green
        case "deleted": case "conflicted": return Kiki.Theme.danger
        case "untracked": return mix(Kiki.Theme.green, Kiki.Theme.muted, 0.45)
        default: return Kiki.Theme.muted
        }
    }
    /// `a` moved `t` of the way towards `b`.
    function mix(a, b, t) { return Qt.rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, 1) }
    function kindLabel(kind) {
        return { folder: "Folder", image: "Image", video: "Video", audio: "Audio", code: "Code", text: "Text", document: "Document", pdf: "PDF document", archive: "Archive", link: "Link", file: "File", other: "Other" }[kind] || "File"
    }
    // ~-shortened display of a URI, as the daemon's Uri::display does. A bare filesystem path —
    // the trash's record of where a file came from is one — is shortened the same way; it used to
    // fall through to the scheme branch, which cut its first two characters off ("ome/t").
    function display(uri, home) {
        if (!uri) return ""
        const i = uri.indexOf("://")
        if (uri.startsWith("file://") || i < 0) {
            let p = i < 0 ? uri : decodeURIComponent(uri.slice(7))
            if (home && p === home) return "~"
            if (home && p.startsWith(home + "/")) return "~/" + p.slice(home.length + 1)
            return p
        }
        return decodeURIComponent(uri.slice(i + 3))
    }
    // Host part of a remote uri ("sftp://homelab/srv" -> "homelab"); empty for local or bare paths.
    function authority(uri) {
        const m = (uri || "").match(/^[a-z0-9+.-]+:\/\/([^/]*)/i)
        return m ? m[1] : ""
    }
    function crumbs(uri, home) {
        const d = display(uri, home)
        const parts = d.split("/").filter(s => s.length)
        if (d.startsWith("/")) parts.unshift("/")
        return parts
    }
}
