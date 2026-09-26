"""Small real media for the flows that show files: a PNG, a two-page PDF, a short MP4, a
Markdown document. Made here rather than kept as fixtures, so the repository carries no binaries
and each is exactly what the test says it is."""
import os, shutil, struct, subprocess, zlib


def png(path, w=320, h=200):
    """A valid PNG: a green field with a darker band, no library needed."""
    rows = bytearray()
    for y in range(h):
        rows.append(0)                       # filter: none
        for x in range(w):
            band = 60 <= y < 140
            rows += bytes((40, 200 if band else 120, 80))
    def chunk(kind, data):
        c = kind + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xffffffff)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(bytes(rows), 6)) + chunk(b"IEND", b""))


def pdf(path, pages=2):
    """A valid PDF of `pages` letter pages, each with one line of text, the xref correct so
    poppler is not asked to repair it."""
    objs = []
    objs.append("<< /Type /Catalog /Pages 2 0 R >>")
    kids = " ".join(f"{3 + i * 2} 0 R" for i in range(pages))
    objs.append(f"<< /Type /Pages /Kids [{kids}] /Count {pages} >>")
    for i in range(pages):
        text = f"BT /F1 36 Tf 72 700 Td (Page {i + 1}) Tj ET"
        objs.append(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {4 + i * 2} 0 R /Resources << /Font << /F1 {3 + pages * 2} 0 R >> >> >>")
        objs.append(f"<< /Length {len(text)} >>\nstream\n{text}\nendstream")
    objs.append("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for n, body in enumerate(objs, 1):
        offsets.append(len(out))
        out += f"{n} 0 obj\n{body}\nendobj\n".encode()
    xref = len(out)
    out += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode()
    for o in offsets:
        out += f"{o:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    with open(path, "wb") as f:
        f.write(out)


def mp4(path, seconds=2):
    """A short test-pattern video with a tone, from ffmpeg (a dependency of the daemon); False
    when ffmpeg is not there."""
    if not shutil.which("ffmpeg"):
        return False
    r = subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", f"testsrc=size=320x240:rate=10:duration={seconds}",
                        "-f", "lavfi", "-i", f"sine=frequency=440:duration={seconds}", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", path],
                       capture_output=True, timeout=60)
    return r.returncode == 0 and os.path.getsize(path) > 1000


MARKDOWN = """# Field notes

A short document with headings at three levels, for the table of contents.

## Monday

Rain all morning. The fence by the north gate wants mending.

### The gate

Two boards gone; the hinge holds.

## Tuesday

Clear. Counted the sheep twice and got two answers.

Setext heading
--------------

A heading the other way, which counts as level two.

```
# not a heading: inside a fence
```

## Wednesday

Nothing to report.
"""
