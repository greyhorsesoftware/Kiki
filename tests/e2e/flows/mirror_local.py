"""Mirror between two local folders, through the workspace (plans 08 and 24; plan 29 A).

The four screens a hand drives — Configure, Preflight, Review, Running — driven over `shell mirror`,
which calls the very functions the buttons call. What is asserted is what the user would see and
what is on disk afterwards: the plan's counts and rows, the report, the Done summary, the
destination tree, and every guard refusing what it is there to refuse.
"""
import json, os, re, shutil, subprocess, time
from harness import snapshot, wait_for

NEEDS = {"shell"}
TITLE = "mirror: scan, plan, run, done, and the guards"

# a.txt 5 bytes, b.txt 7, sub/c.txt 7 — 19 to copy, 5 entries in all.
TREE = {"a.txt": "alpha", "b.txt": "bravo!!", "sub": {"c.txt": "charlie"}, "hollow": {}}

# The built-in filter rules: eight names, each an `is`, and not the pattern `starts with .` they
# used to be. A dotted name is mirrored unless the rules name it, because `.htaccess` and
# `.well-known/acme-challenge/` are exactly what a website mirror exists to carry.
NAMED_DEFAULTS = [".git", ".gitignore", ".DS_Store", ".env", ".idea", ".vscode", "Thumbs.db", "node_modules", "__pycache__"]
NAMED_LINES = "\n".join("matches " + n for n in NAMED_DEFAULTS)


def ws(sh, action="state"):
    """Drive the workspace and read it back: one call is both."""
    try:
        return json.loads(sh.call("mirror", action) or "{}")
    except json.JSONDecodeError:
        return {}


def wait_ws(sh, pred, action="state", timeout=20):
    return wait_for(lambda: (lambda s: s if s and pred(s) else None)(ws(sh, action)), timeout)


def open_pair(ctx, spec):
    """Two folders side by side and the workspace open on Configure. Returns (root, src, dst)."""
    sh = ctx.shell
    root = ctx.fixture(spec)
    src, dst = os.path.join(root, "src"), os.path.join(root, "dst")
    os.makedirs(src, exist_ok=True)
    os.makedirs(dst, exist_ok=True)
    sh.call("split", "on")
    sh.call("focusPane", "left")
    sh.open("file://" + src)
    sh.call("focusPane", "right")
    sh.open("file://" + dst)
    ws(sh, "open")
    wait_ws(sh, lambda s: s.get("open") and s.get("screen") == "configure")
    return root, src, dst


def preflight(sh):
    """Preflight, and the Review screen it lands on (or Configure again when it refuses)."""
    ws(sh, "preflight")
    return wait_ws(sh, lambda s: s.get("screen") in ("review", "configure")) or ws(sh)


def mirror(sh):
    """The Mirror button, and the run once it is over."""
    ws(sh, "run")
    return wait_ws(sh, lambda s: (s.get("run") or {}).get("state") in ("done", "failed", "cancelled") or s.get("confirm")) or ws(sh)


def fresh_report(sh):
    """Save report…, and the text it brings back for the scan that is on screen NOW — the last
    one's is still in hand until this one lands, and would answer for it."""
    prev = ws(sh).get("report", "")
    ws(sh, "report")
    return (wait_ws(sh, lambda s: s.get("report") and s.get("report") != prev) or ws(sh)).get("report", "")


def rows_of(st, kind=None):
    return sorted(r["rel"] for r in st.get("rows", []) if kind is None or r["action"] == kind)


def rules_of(st):
    return [(r["kind"], r["value"]) for r in (st.get("rules") or [])]


def run(ctx):
    c, sh = ctx.checks, ctx.shell

    # ---------------------------------------------------------------- scan → plan → run → done
    root, src, dst = open_pair(ctx, {"src": TREE, "dst": {}})
    st = ws(sh)
    c.check("the workspace opens on Configure, over both folders",
            st.get("screen") == "configure" and st.get("local") == "file://" + src and st.get("remote") == "file://" + dst, st)
    c.check("uploading, keeping the filter rules, deleting nothing — the safe defaults",
            (st.get("direction"), st.get("deleteExtras"), st.get("applyFilters"), st.get("detector")) == ("upload", False, True, "auto"), st)

    st = preflight(sh)
    c.check("Preflight lands on Review with a plan of every entry", st.get("screen") == "review" and st.get("planCount") == 5, st)
    c.check("five new, nothing changed, nothing to delete, 19 bytes to move",
            (st["counts"].get("new"), st["counts"].get("changed"), st["counts"].get("equal"), st["counts"].get("deletes"), st["counts"].get("copyBytes")) == (5, 0, 0, 0, 19), st.get("counts"))
    c.check("the two folders are made and the three files copied",
            rows_of(st, "mkdir") == ["hollow", "sub"] and rows_of(st, "copy") == ["a.txt", "b.txt", "sub/c.txt"], rows_of(st))
    c.check("every actionable row starts checked, and none has run yet",
            all(r["checked"] for r in st["rows"]) and all(r["state"] == "pending" for r in st["rows"]), st.get("rows"))
    # The footer used to add the two copy counts as text: 3 new and 1 changed read as "31 copy".
    c.check("the footer counts the copies, it does not spell them out",
            st.get("planSummary", "").strip() == "5 copy · 0 delete · 19 B to transfer · 0 filtered out", st.get("planSummary"))

    ws(sh, "report")
    st = wait_ws(sh, lambda s: s.get("report")) or ws(sh)
    text = st.get("report", "")
    c.check("the report names the direction, both roots and the detector",
            text.startswith("kiki mirror report") and "Direction:         upload" in text
            and ("Master (source):   file://" + src) in text and ("Replica (dest):    file://" + dst) in text
            and "Detector:          size+mtime" in text, text[:300])
    c.check("its summary counts agree with the screen", "Summary: 3 to copy, 0 to delete, 0 unchanged  (replica had 0 items)" in text, text[:600])
    c.check("and it has a line per action, with what each side holds",
            re.search(r"Copy/New \| a\.txt \| master: 5b @ \d{4}-\d\d-\d\d .* \| replica: — \| 5b", text) is not None
            and "Mkdir/New | sub |" in text and "sub/c.txt" in text, [l for l in text.splitlines() if "|" in l][:6])

    st = mirror(sh)
    c.check("the run ends done, having done everything it said it would",
            st["run"]["state"] == "done" and st["run"]["done"] == st["run"]["total"] == 5 and st["run"]["bytes"] == 19, st.get("run"))
    c.check("the Done summary says what came of it", st.get("summary") == "Mirror complete · 3 copied · 0 deleted · 19 B", st.get("summary"))
    c.check("every row of the table ended done", all(r["state"] == "done" for r in st["rows"]), [(r["rel"], r["state"]) for r in st.get("rows", [])])
    c.same_tree("the destination is the source, entry for entry", snapshot(src), snapshot(dst))

    # ---------------------------------------------------------------- a second run has nothing to do
    ws(sh, "close")
    st = ws(sh, "open")
    c.check("closing and coming back opens Configure again, not the table of the last run",
            st.get("screen") == "configure" and st.get("planCount") == 0 and st.get("run") is None, st)
    st = preflight(sh)
    c.check("the second run finds nothing to do — the copies kept their modification times",
            (st["counts"].get("new"), st["counts"].get("changed"), st["counts"].get("equal")) == (0, 0, 5), st.get("counts"))
    c.check("and says so as plainly on the footer", st.get("planSummary", "").strip().startswith("0 copy · 0 delete"), st.get("planSummary"))
    c.check("with every row unchanged", rows_of(st, "skip") == ["a.txt", "b.txt", "hollow", "sub", "sub/c.txt"], rows_of(st))

    # One file edited: exactly one copy, and only that one.
    with open(os.path.join(src, "b.txt"), "w") as f:
        f.write("bravo again!")
    os.utime(os.path.join(src, "b.txt"), (time.time(), time.time()))
    ws(sh, "back")
    st = preflight(sh)
    c.check("editing one file plans exactly one changed copy",
            (st["counts"].get("changed"), st["counts"].get("new")) == (1, 0) and rows_of(st, "copy") == ["b.txt"], st.get("counts"))
    st = mirror(sh)
    c.check("which runs, and nothing else does", st["run"]["state"] == "done" and st["run"]["total"] == 1, st.get("run"))
    c.same_tree("leaving the two folders the same again", snapshot(src), snapshot(dst))

    # ---------------------------------------------------------------- the clock offset, by hand
    # Two machines whose clocks disagree: the same files, stamped two hours earlier on the
    # destination. The engine compares `master.mtime − offset − replica.mtime`, so +2 is what
    # reconciles a destination two hours BEHIND — and −2 is four hours out, not none. A sign the
    # wrong way about re-copies the whole tree on every run, so both are asserted.
    ws(sh, "close")
    SAME = {"one.txt": "one", "two.txt": "two", "three.txt": "three"}
    root, src, dst = open_pair(ctx, {"src": dict(SAME), "dst": dict(SAME)})
    for rel in SAME:
        t = os.path.getmtime(os.path.join(src, rel)) - 7200
        os.utime(os.path.join(dst, rel), (t, t))

    st = ws(sh, "set offset 0")
    c.check("the offset can be taken off automatic and set by hand",
            (st.get("offsetAuto"), st.get("offsetHours"), st.get("offsetText")) == (False, 0, "Both sides' clocks read the same"), st)
    st = preflight(sh)
    c.check("with no offset allowed for, every file reads two hours out of date",
            (st["counts"].get("changed"), st["counts"].get("equal")) == (3, 0), st.get("counts"))

    ws(sh, "back")
    st = ws(sh, "set offset 2")
    c.check("and the row says which way the two hours go, in words",
            st.get("offsetText") == "The destination's clock is 2 hours behind the source", st)
    st = preflight(sh)
    c.check("allowing for them, there is nothing to copy",
            (st["counts"].get("changed"), st["counts"].get("equal")) == (0, 3), st.get("counts"))
    text = fresh_report(sh)
    c.check("the report names the offset the plan was made with, and which way it goes",
            "Clock offset:      7200000 ms (manual, subtracted from master mtime)" in text,
            [l for l in text.splitlines() if "offset" in l])

    # The button itself (owner: "save report… does nothing"): it opens kiki's own chooser in
    # save mode, with a name offered; choosing a file writes the report there and says so.
    ws(sh, "saveReport")
    c.check("Save report… opens the chooser to save", sh.wait_state(lambda s: s.get("dialogs", {}).get("portal")) is not None, sh.state())
    out = os.path.join(ctx.root, "report.txt")
    ws(sh, "save file://" + out)
    saved = wait_ws(sh, lambda s: s.get("saved"))
    c.check("and the file chosen has the report in it", saved is not None and os.path.exists(out) and "Clock offset" in open(out).read(),
            (saved or {}).get("saved") if saved else "no answer")
    c.check("which the toast says", saved is not None and saved.get("saved", "").startswith("Report saved to "), (saved or {}).get("saved"))

    ws(sh, "back")
    ws(sh, "set offset -2")
    st = preflight(sh)
    c.check("the sign is the whole of it: two hours the wrong way is four hours out",
            st["counts"].get("changed") == 3, st.get("counts"))
    ws(sh, "back")
    st = ws(sh, "set offset 25")
    c.check("and an offset past a day either way is refused, leaving the hours as they were",
            (st.get("offsetHours"), st.get("offsetAuto")) == (-2, False), st)

    st = ws(sh, "set offset auto")
    c.check("automatic again, and it says where the number will come from",
            st.get("offsetAuto") is True and st.get("offsetText") == "Determined automatically from files present on both sides", st)
    st = preflight(sh)
    c.check("left to itself the scan measures those same two hours", st["counts"].get("equal") == 3, st.get("counts"))
    text = fresh_report(sh)
    c.check("and the report says so, measured rather than given",
            "Clock offset:      7200000 ms (auto, subtracted from master mtime)" in text,
            [l for l in text.splitlines() if "offset" in l])

    # ---------------------------------------------------------------- a row nobody wants
    ws(sh, "close")
    root, src, dst = open_pair(ctx, {"src": {"one.txt": "1", "two.txt": "2"}, "dst": {}})
    st = preflight(sh)
    i = next(r["i"] for r in st["rows"] if r["rel"] == "two.txt")
    ws(sh, "check %d off" % i)
    st = wait_ws(sh, lambda s: not next((r["checked"] for r in s["rows"] if r["rel"] == "two.txt"), True)) or ws(sh)
    c.check("a row can be taken out of the plan",
            not next(r["checked"] for r in st["rows"] if r["rel"] == "two.txt") and st["counts"]["new"] == 2, st.get("rows"))
    st = mirror(sh)
    c.check("and then it is not run", st["run"]["state"] == "done" and st["run"]["total"] == 1, st.get("run"))
    c.check("so only the row that was left arrives", sorted(os.listdir(dst)) == ["one.txt"], os.listdir(dst))

    # ---------------------------------------------------------------- deletes, opt in
    ws(sh, "close")
    kept = {"keep%d.txt" % i: "keep" for i in range(4)}
    root, src, dst = open_pair(ctx, {"src": dict(kept), "dst": dict(kept, **{"gone.txt": "gone", "old": {"stale.txt": "stale"}})})
    st = preflight(sh)
    c.check("with deletes off, what is only on the destination is left alone",
            st["counts"].get("deletes") == 0 and not rows_of(st, "delete"), st.get("counts"))
    before = snapshot(dst)
    st = mirror(sh)
    c.check("and a run does not touch it", st["run"]["state"] == "done", st.get("run"))
    c.same_tree("nothing on the destination was removed", before, snapshot(dst))

    ws(sh, "close")
    ws(sh, "open")
    st = ws(sh, "set deletes on")
    c.check("deletes are a switch on the Configure screen", st.get("deleteExtras") is True, st)
    st = preflight(sh)
    c.check("now the extras are planned for deletion, children before parents",
            st["counts"].get("deletes") == 3 and rows_of(st, "delete") == ["gone.txt", "old/stale.txt"] and rows_of(st, "rmdir") == ["old"], rows_of(st))
    c.check("the plan says how much of the destination that is", st["counts"].get("replicaEntries") == 7, st.get("counts"))
    st = mirror(sh)
    c.check("the run deletes them", st["run"]["state"] == "done" and (st["run"]["result"] or {}).get("deletes") == 3, st.get("run"))
    c.check("and the Done summary counts them too", st.get("summary", "").startswith("Mirror complete · 0 copied · 3 deleted"), st.get("summary"))
    c.same_tree("leaving the destination exactly the source", snapshot(src), snapshot(dst))
    # Every delete and every run is written down (plan 08), in the state directory this run was
    # given — the engine used to build its own from XDG_STATE_HOME, so a test run appended its
    # mirrors to the audit log in the home of whoever ran it.
    audit = os.path.join(os.environ["KIKI_STATE_DIR"], "audit.log")
    lines = open(audit).read().splitlines() if os.path.exists(audit) else []
    c.check("every delete is written to the audit log, by name",
            sorted(l.split(" ", 2)[2] for l in lines if " mirror-delete " in l) == ["gone.txt", "old", "old/stale.txt"],
            [l for l in lines if "mirror-delete" in l][-4:])
    c.check("and the run is summed up at the end of it",
            any(("MIRROR file://" + src) in l and "-> file://" + dst in l and "copies=0 deletes=3 bytes=0 skipped=0" in l for l in lines), lines[-2:])

    # ---------------------------------------------------------------- guard: the blast radius
    ws(sh, "close")
    extras = {"x%d.txt" % i: "x" for i in range(5)}
    root, src, dst = open_pair(ctx, {"src": {"keep.txt": "keep"}, "dst": dict(extras, **{"keep.txt": "keep"})})
    ws(sh, "set deletes on")
    st = preflight(sh)
    c.check("five of the six items on the destination are extras", (st["counts"].get("deletes"), st["counts"].get("replicaEntries")) == (5, 6), st.get("counts"))
    before = snapshot(dst)
    st = mirror(sh)
    c.check("a delete that big is asked about before anything is run", st.get("confirm") is True and (st.get("run") or None) is None, st)
    c.check("and it says how big, in items and in per cent",
            st.get("confirmText") == "This will delete 5 of 6 items (83%) on the destination. Proceed?", st.get("confirmText"))
    st = ws(sh, "confirm no")
    c.check("declining leaves the plan on screen, unrun", st.get("confirm") is False and st.get("screen") == "review" and st.get("run") is None, st)
    c.same_tree("and the destination untouched", before, snapshot(dst))
    ws(sh, "run")
    wait_ws(sh, lambda s: s.get("confirm"))
    ws(sh, "confirm yes")
    st = wait_ws(sh, lambda s: (s.get("run") or {}).get("state") in ("done", "failed")) or ws(sh)
    c.check("confirmed, it runs and takes the five", st["run"]["state"] == "done" and (st["run"]["result"] or {}).get("deletes") == 5, st.get("run"))
    c.same_tree("the destination is the source", snapshot(src), snapshot(dst))

    # ---------------------------------------------------------------- guard: an empty source
    ws(sh, "close")
    root, src, dst = open_pair(ctx, {"src": {}, "dst": {"precious.txt": "precious", "more": {"also.txt": "also"}}})
    ws(sh, "set deletes on")
    before = snapshot(dst)
    st = preflight(sh)
    c.check("an empty source with deletes on is refused, before anything is scanned into a plan",
            st.get("screen") == "configure" and "refusing to delete the entire replica" in st.get("status", ""), (st.get("screen"), st.get("status")))
    c.same_tree("and the destination still has everything", before, snapshot(dst))
    st = ws(sh, "set deletes off")
    st = preflight(sh)
    c.check("with deletes off the same pair is merely nothing to do", st.get("screen") == "review" and st.get("planCount") == 0, st)

    # ---------------------------------------------------------------- guard: a filtered name is never an extra
    ws(sh, "close")
    root, src, dst = open_pair(ctx, {"src": {"page.html": "<p>"}, "dst": {".git": {"HEAD": "ref"}, "node_modules": {"left.js": "//"}}})
    ws(sh, "set deletes on")
    st = preflight(sh)
    c.check("what the filter rules name is not on the destination as far as the plan is concerned",
            st["counts"].get("deletes") == 0 and st["counts"].get("filtered") == 2, st.get("counts"))
    st = mirror(sh)
    c.check("so a run with deletes on leaves it alone",
            st["run"]["state"] == "done" and sorted(os.listdir(dst)) == [".git", "node_modules", "page.html"], os.listdir(dst))

    # ---------------------------------------------------------------- the filter rules themselves
    # Edit rules… (plan 08). The rules are what a mirror does NOT transfer, and they are read at
    # scan, so every one of these is asserted on the plan and on the destination tree rather than
    # on what the dialog says about itself.
    ws(sh, "close")
    LEAKY = {"page.html": "<p>", ".htaccess": "Redirect /", ".well-known": {"acme-challenge": {"token": "tok"}},
             ".env": "SECRET=1", ".git": {"HEAD": "ref"}, "node_modules": {"left.js": "//"}, "notes.tmp": "scratch"}
    WEB = [".htaccess", ".well-known/acme-challenge/token"]
    root, src, dst = open_pair(ctx, {"src": dict(LEAKY), "dst": {}})
    st = wait_ws(sh, lambda s: s.get("rules"), "rules") or ws(sh)
    c.check("out of the box the rules are the built-in eight, named one by one",
            rules_of(st) == [("matches", n) for n in NAMED_DEFAULTS] and st.get("rulesDefault") is True, st.get("rules"))

    st = preflight(sh)
    c.check("so the three named trees are filtered and the dotted files a website needs are copied",
            rows_of(st, "copy") == WEB + ["notes.tmp", "page.html"] and st["counts"].get("filtered") == 3, (rows_of(st), st.get("counts")))
    st = mirror(sh)
    c.check("`.htaccess` and the ACME challenge arrive; `.env`, `.git` and node_modules do not",
            st["run"]["state"] == "done" and sorted(os.listdir(dst)) == [".htaccess", ".well-known", "notes.tmp", "page.html"], os.listdir(dst))

    # A rule added: the same save the dialog's Done does, and the next scan honours it.
    ws(sh, "close")
    shutil.rmtree(dst); os.makedirs(dst)
    ws(sh, "open")
    st = ws(sh, "rules-set " + NAMED_LINES + "\nendsWith .tmp")
    st = wait_ws(sh, lambda s: len(s.get("rules") or []) == 9) or ws(sh)
    c.check("a rule can be added, and it is no longer the built-in set",
            rules_of(st)[-1:] == [("endsWith", ".tmp")] and st.get("rulesDefault") is False, st)
    st = preflight(sh)
    c.check("and the scratch file it names is skipped too",
            rows_of(st, "copy") == WEB + ["page.html"] and st["counts"].get("filtered") == 4, (rows_of(st), st.get("counts")))
    st = mirror(sh)
    c.check("leaving the page and the two the server needs",
            sorted(os.listdir(dst)) == [".htaccess", ".well-known", "page.html"], os.listdir(dst))

    # `starts with .` added by hand: every dotted name goes, `.htaccess` and the challenge with it.
    # This is the rule that used to be built in, and it is the one that decides whether a website
    # mirror ever uploads its `.htaccess` — the owner should see it working both ways.
    ws(sh, "close")
    shutil.rmtree(dst); os.makedirs(dst)
    ws(sh, "open")
    ws(sh, "rules-set " + NAMED_LINES + "\nendsWith .tmp\nstartsWith .")
    wait_ws(sh, lambda s: len(s.get("rules") or []) == 10) or ws(sh)
    st = preflight(sh)
    c.check("with `starts with .` added, every dotted name is skipped — the ACME challenge too",
            rows_of(st, "copy") == ["page.html"] and st["counts"].get("filtered") == 6, (rows_of(st), st.get("counts")))
    st = mirror(sh)
    c.check("and only the page arrives", sorted(os.listdir(dst)) == ["page.html"], os.listdir(dst))

    # The check on the Configure screen still turns the lot off, rules or no rules.
    ws(sh, "close")
    shutil.rmtree(dst); os.makedirs(dst)
    ws(sh, "open")
    st = ws(sh, "set filters off")
    c.check("the rules are still there with the check off — they are edited either way",
            st.get("applyFilters") is False and len(st.get("rules") or []) == len(NAMED_DEFAULTS) + 2, st.get("rules"))
    st = preflight(sh)
    c.check("and with it off nothing is filtered at all",
            st["counts"].get("filtered") == 0
            and rows_of(st, "copy") == [".env", ".git/HEAD"] + WEB + ["node_modules/left.js", "notes.tmp", "page.html"], (rows_of(st), st.get("counts")))
    st = mirror(sh)
    c.check("so the whole tree is copied, dotted names and all",
            sorted(os.listdir(dst)) == [".env", ".git", ".htaccess", ".well-known", "node_modules", "notes.tmp", "page.html"], os.listdir(dst))

    # The rules live in the daemon's filters.toml, not in this window: closing the workspace and
    # opening it again reads them back, and the scan that follows reads them again for itself.
    ws(sh, "close")
    shutil.rmtree(dst); os.makedirs(dst)
    st = ws(sh, "open")
    ws(sh, "set filters on")
    st = wait_ws(sh, lambda s: s.get("rules"), "rules") or ws(sh)
    c.check("the rules come back from the daemon after the workspace was closed",
            rules_of(st) == [("matches", n) for n in NAMED_DEFAULTS] + [("endsWith", ".tmp"), ("startsWith", ".")], st.get("rules"))
    st = preflight(sh)
    c.check("and the scan reads them for itself", st["counts"].get("filtered") == 6, st.get("counts"))

    # Restore defaults: the file goes, and the built-in rules are what is read next. The list the
    # dialog previews is the daemon's own — it is in every reply — so there is nothing to drift.
    ws(sh, "back")
    ws(sh, "rules-defaults")
    st = wait_ws(sh, lambda s: s.get("rulesDefault") is True) or ws(sh)
    c.check("Restore defaults puts the built-in eight back",
            rules_of(st) == [("matches", n) for n in NAMED_DEFAULTS], st.get("rules"))
    st = preflight(sh)
    c.check("and `.htaccess` is mirrored once more", rows_of(st, "copy") == WEB + ["notes.tmp", "page.html"], rows_of(st))

    # Saving while a plan is on screen: that plan was made with the old rules, so the workspace
    # goes back to the form rather than offering to run something it no longer means.
    ws(sh, "rules-set startsWith .")
    st = wait_ws(sh, lambda s: s.get("screen") == "configure") or ws(sh)
    c.check("a plan made with the old rules is not left on screen after they change",
            st.get("screen") == "configure" and len(st.get("rules") or []) == 1, (st.get("screen"), st.get("rules")))

    # The dialog itself, once, for whoever wants to see what the checks can only describe.
    ws(sh, "rules-defaults")
    wait_ws(sh, lambda s: s.get("rulesDefault") is True)
    st = ws(sh, "rules-edit")
    st = wait_ws(sh, lambda s: len((s.get("rulesDialog") or {}).get("rules") or []) == 8) or ws(sh)
    c.check("Edit rules… opens on the rules that are saved",
            (st["rulesDialog"] or {}).get("open") is True
            and [(r["kind"], r["value"]) for r in st["rulesDialog"]["rules"]] == [("matches", n) for n in NAMED_DEFAULTS], st.get("rulesDialog"))
    if shutil.which("grim"):
        time.sleep(0.4)
        subprocess.run(["grim", os.path.join(os.environ.get("KIKI_E2E_OUT", "/tmp"), "mirror-rules.png")], capture_output=True)
    st = ws(sh, "rules-cancel")
    c.check("and Cancel leaves the rules exactly as they were",
            (st["rulesDialog"] or {}).get("open") is False and st.get("rulesDefault") is True, st.get("rulesDialog"))

    # ---------------------------------------------------------------- guard: a folder into itself
    ws(sh, "close")
    root = ctx.fixture({"tree": {"a.txt": "a", "inner": {"b.txt": "b"}}})
    outer, inner = os.path.join(root, "tree"), os.path.join(root, "tree", "inner")
    sh.call("focusPane", "left"); sh.open("file://" + outer)
    sh.call("focusPane", "right"); sh.open("file://" + inner)
    ws(sh, "open")
    before = snapshot(outer)
    st = preflight(sh)
    c.check("a folder mirrored into something inside it is refused",
            st.get("screen") == "configure" and "inside the source" in st.get("status", ""), (st.get("screen"), st.get("status")))
    st = ws(sh, "set direction download")
    st = preflight(sh)
    c.check("and so is the other way about", st.get("screen") == "configure" and "inside the destination" in st.get("status", ""), (st.get("screen"), st.get("status")))
    sh.call("focusPane", "right"); sh.open("file://" + outer)
    ws(sh, "set direction upload")
    st = preflight(sh)
    c.check("as is a folder mirrored onto itself", st.get("screen") == "configure" and "the same folder" in st.get("status", ""), (st.get("screen"), st.get("status")))
    c.same_tree("nothing was copied into itself", before, snapshot(outer))

    # ---------------------------------------------------------------- guard: no run without a plan
    st = ws(sh, "run")
    c.check("and there is no running a plan nobody has seen", st.get("screen") == "configure" and st.get("run") is None, st)
    ws(sh, "close")
    c.check("closing puts the two panes back", sh.state().get("split") is True and ws(sh).get("open") is False, ws(sh))

    # The way in a hand uses: the keymap action behind Ctrl+M and the toolbar button, which is one
    # function, so this is that button too (plan 24).
    sh.call("action", "mirror")
    st = wait_ws(sh, lambda s: s.get("open")) or ws(sh)
    c.check("the mirror action opens the workspace, uploading from the local side",
            st.get("screen") == "configure" and st.get("direction") == "upload" and st.get("local", "").startswith("file://"), st)
    sh.call("action", "mirror")
    st = wait_ws(sh, lambda s: not s.get("open")) or ws(sh)
    c.check("and closes it again, back at Configure", st.get("open") is False and st.get("screen") == "configure", st)
