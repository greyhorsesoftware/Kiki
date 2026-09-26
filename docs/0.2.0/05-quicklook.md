# 05 — Quick Look

**Status:** **built, 2026-09-25**, in four parts by four agents with one integrator — the daemon's verbs (`quicklook.rs`: `ReadText`, `PdfInfo`, `PdfPage`, `QuickLookFetch`, errors 1300–1320), the window with images and text (`QuickLookWindow/Image/Text.qml`, the Shell's `Space` and `quickLook` IPC), Markdown with the table of contents (`QuickLookMarkdown.qml`, `markdown_toc.js`), PDF and video (`QuickLookPdf/Video.qml`, `qt6-multimedia` and `poppler` as dependencies); `tst_QuickLook*` (58 tests), `kikid/tests/quicklook.rs`, the e2e flow `quick_look.py`, a photograph per kind in es and ja. Planned the same day (owner: "when user presses space, a floating window w/ close
box should show. it needs to support previewing images, pdfs, videos and markdown documents. for
markdown, I want a collapsable table of contents on the left hand side… also needs to support
previewing remote stuff too". A double-click keeps opening the file in its application — the
owner considered sending it here too and withdrew it the same day.)

## What it is

A file's own window, opened with `Space` on a file: a floating window of its own with a close box, showing the file itself — an image at its size, a
PDF page by page, a video playing, a Markdown document rendered with its headings down the left
as a table of contents that folds away. It shows a file on a server the same way, fetched
first. It is a look, not an editor: nothing is written. A double-click or `Enter` on a local file
opens it in its application (`03-fixes.md`); on a file on a server it opens Quick Look (owner,
2026-09-25: "double click should just open the quicklook window for remote") — nothing on a
server is opened in an application from kiki until remote editing is built (`docs/0.3.0`).

## Decisions

1. **A window of its own, not a panel over the list.** Settings and the shortcuts are panels
   inside the main window; a preview is something you put beside the list and keep looking at
   while you move on, so it is a Quickshell `FloatingWindow` — the compositor moves and tiles
   it. **Above everything** (owner): on Hyprland that is a window rule — `float` and `pin` on
   the window's title, which ends in a constant "— Quick Look" for the rule to match whatever
   the language — installed by Settings › Omarchy with the chooser's rules (`integrate.rs`,
   `hypr_block`). A close box top-right, `Esc` or `Space` again close it. Its size is remembered; an image opens no bigger than it is and no bigger
   than four-fifths of the screen.
2. **One window, following the selection.** `Space` on another file, or `j`/`k`, `←`/`→`
   while the window has the focus, shows that file in the same window (the gallery's "step",
   not a second window). The title is the file's name; the window says where the file is when
   that is not the pane's folder (a search result, a server).
3. **Kinds, and how each is shown.**
   - **Image** — the file, through `Image` with `asynchronous: true` and `autoTransform` (EXIF
     rotation); the thumbnail stands in until it is decoded, as the gallery does. Zoom with the
     wheel or `+`/`-`, `0` fits, `1` is actual size — the gallery's keys, so nothing new to learn.
   - **Video** — `QtMultimedia`'s `MediaPlayer` + `VideoOutput` (the `qt6-multimedia` package,
     FFmpeg backend; it becomes a dependency — `packaging/PKGBUILD`, the AUR one, the desktop
     entry untouched). Play/pause on `Space`… which is taken; so `k` is play/pause as in mpv,
     with a scrub bar and a time. Sound follows the desktop's default output. The gallery keeps
     showing a poster frame for now; once the player exists it can take it too (a follow-up,
     not this plan).
   - **PDF** — **pages rendered by the daemon**, not `QtQuick.Pdf`. `QtQuick.Pdf` on Arch is
     part of `qt6-webengine` (~250 MB, Chromium) — too much to ask of every install for one
     view. The daemon already renders a PDF's first page for thumbnails with `pdftoppm`
     (`thumbs::pdf_page`); it grows a `PdfPage { uri, page, width }` verb that renders any page
     at a given width into the thumbnail cache (keyed by file, mtime, page, width) and answers
     with the PNG's path and the page count. The window is a `ListView` of pages, each an
     `Image` asked for as it scrolls into view, re-rendered when the window is resized (debounced).
     Zoom is the page width. What this gives up: text selection and find-in-document; a preview
     does not need them. `poppler` moves from optional to a dependency.
   - **Markdown** — `TextEdit` read-only with `textFormat: TextEdit.MarkdownText` (the inspector
     already renders a head this way; here the whole file, through a `ReadText { uri }` verb
     capped at 4 MB, said when it was cut). Relative images resolve against the file's folder
     for a local file (`baseUrl`); for a fetched remote file they are left broken rather than
     fetched one by one — noted on screen. Code blocks in the mono font; the rest in the UI
     font at reading size; a maximum line width so a wide window does not give 200-character
     lines.
   - **The table of contents** — the headings (`#` … `######`, and setext `===`/`---`), parsed
     from the source in QML, in a column on the left, indented by level; the one whose heading
     is at the top of the view is marked as the document scrolls. A click scrolls to the heading:
     the heading's text is found in the rendered document (`getText`) and `positionToRectangle`
     gives where to go. A chevron folds the column to a thin strip (the state remembered, in
     `view.quickLookToc`); a document with no headings has no column.
   - **Text and code** — the file as text (`ReadText`), mono; **code in colour** (added
     2026-09-25, owner: "can we add syntax highlighting … html, js, rs, c, java etc"): the daemon
     parses the text with `syntect`'s grammars (~50 languages by file name, pure Rust) and
     answers `runs` — `[offset, length, kind]` in UTF-16 units, kind one of keyword, string,
     comment, number, type, function, attribute, punctuation — and the window shows rich text
     with a span per run coloured from `Theme.code`, the theme's own colours. The daemon says
     what a token is, the window what it looks like. Files over 512 KB come plain. Anything the window cannot show — a spreadsheet, an archive, a binary — shows
     the file's icon, name, kind and size, and an **Open with…** button that is the existing
     menu. It never starts an application by itself.
4. **Remote files: fetched, then the same.** A file on a server goes through
   `fetched::bring` — a copy job into `~/.cache/kiki/open/<moment>/`, in the orb, cancellable,
   its wire logged — and the window shows the local copy once it is there, with the job's
   progress in the window until then. Two differences from opening: the fetched folder is not
   watched for saves (a preview writes nothing — `fetch(uris, watch: false)`), and a file over a
   size the setting names (`view.quickLookFetchLimit`, 200 MB) is not fetched without asking;
   the fetch is a hidden job — the window shows its progress, the orb and the bar say nothing
   and there is no Undo — and the copy is dropped when the look is over (`QuickLookDrop` on close or on moving to another
   file — owner: "quicklook cleans up after itself I assume?"), and the daemon empties the
   folder at start for whatever a crash left —
   the window says how big it is and offers the fetch. Video over SFTP is played from the copy
   once whole; streaming through the plugins' partial reads is a later thing, not this plan.
5. **Nothing rebindable.** `Space` opens and closes; the keys inside are the gallery's and mpv's
   and are contextual like the arrows (`04-keys.md`). The shortcuts window's intro names `Space`.
6. **Localized from the first line** (owner: "make sure the quicklook window is localized
   too"; `02-localization.md`). Every word the window shows comes from the catalogs through
   `Kiki.T` — the close box's tip, "Open with…", "Fetching… {n} of {total}", "{size} — fetch
   it?", "Page {n} of {total}", "Contents", "no headings", "cut at {n} MB", the time on the
   scrub bar — in `en`, `es` and `ja` together, never English first and the rest later; the
   guard (`tests/i18n_check.py`) fails the build on a bare literal in the window. Numbers, sizes
   and times go through `Format` and the locale (`1,5 MB` in Spanish). Nothing is laid out for
   English widths: the table-of-contents column and the fetch offer are measured from the
   longest of the three languages (as the info panel's labels are), and Japanese needs the CJK
   fallback font, already an optional dependency. The daemon's part says numbers, not words —
   a page that will not render, a fetch that fails, a file that vanished, a file too big — each
   a `VfsError::Said` with its `error.<n>` sentence in the three catalogs. A file's own text
   (a Markdown document, a video's audio) is of course shown as it is.

## Layers

| | | |
|---|---|---|
| L1 | **The window**: `QuickLookWindow.qml` (FloatingWindow, close box, title, remembered size), opened by `Space` on a file from every view that lists files; follows the selection; `Esc`/`Space` close; the unsupported-kind face with Open with…; images with the gallery's zoom keys. | 1 day |
| L2 | **Markdown** with the table of contents: `ReadText`, the parser, the column, the fold, the jump, the "at the top" mark. | 1 day |
| L3 | **PDF**: `PdfPage` in the daemon (render, cache, page count, numbered errors), the page list, zoom by width, re-render on resize. | 1 day |
| L4 | **Video**: `qt6-multimedia`, player, scrub bar, `k`, the dependency in both PKGBUILDs. | ½ day |
| L5 | **Remote**: `fetch(watch: false)`, progress in the window, the size guard and its setting. | ½ day |
| L6 | **Tests and words**: `tst_QuickLook` (the TOC parser on setext and ATX headings, the fold, the close box, the keys, the unsupported face), Rust tests for `PdfPage` and `ReadText`'s cap, an e2e flow `quick_look.py` (Space on each kind; a file over SFTP fetched and shown; `Space` starts no application — `KIKI_OPEN_WITH` records what would have run), the window photographed in Spanish and Japanese with each kind on show (the photographs flow gains a `quick-look-*` shot per kind) and what clips fixed by layout; the catalog keys in all three languages. | 1 day |

About **5 working days**. L1 alone is a usable Quick Look for images and text.

## Acceptance

- `Space` on a JPEG opens a floating window with the picture at its size, a close box, the
  name as the title; `Space` again closes it; `j` in the list moves on and the window follows.
- `Space` on a `.md` opens it rendered, headings down the left; clicking a heading
  scrolls to it; the chevron folds the column and it stays folded next time.
- A 40-page PDF scrolls page by page, later pages rendering as they come into view; widening
  the window sharpens them.
- An MP4 plays with sound; `k` pauses; the scrub bar moves it.
- `Space` on a file over SFTP shows the fetch's progress, then the file; nothing is left
  watched; a 500 MB file over SFTP is offered, not fetched.
- `Space` starts no application: `KIKI_OPEN_WITH`'s log is empty after the flow; a
  double-click on a local file still opens it in its application, on a server's file it opens
  Quick Look.
- The window photographed in Spanish and Japanese with an image, a PDF, a video, a Markdown
  document with its table of contents, the fetch offer and the unsupported-kind face: nothing
  clipped, no English word on a Spanish screen (the guard proves no key is missing; the
  photographs prove the layout).
- `make lint`, `cargo test`, `make test-qml`, `tests/e2e/run.sh` green.
