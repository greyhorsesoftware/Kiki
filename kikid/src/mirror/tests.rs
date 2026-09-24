//! The mirror's tests: filters, detectors, the pure diff, the guards and a real run.

use super::*;

fn e(rel: &str, is_dir: bool, size: u64, mtime: u64) -> (String, Entry) {
    (rel.to_string(), Entry { path: 0, is_dir, size, mtime_ms: mtime, digest: None })
}
fn spec(delete: bool) -> Spec {
    Spec {
        master: Uri::parse("/m").unwrap(),
        replica: Uri::parse("/r").unwrap(),
        direction: Direction::Upload,
        delete_extras: delete,
        blast_radius: 0.5,
        confirmed_large_delete: false,
        clock_offset_ms: 0,
        clock_offset_auto: false,
        detector: Detector::SizeMtime,
        modified_within_ms: None,
        apply_filters: false,
    }
}
fn kinds(p: &Plan) -> Vec<(String, ActionKind, Reason)> {
    p.actions.iter().map(|a| (a.rel.to_string(), a.kind, a.reason)).collect()
}

#[test]
fn diff_matrix() {
    let m: SideMap =
        [e("a", true, 0, 0), e("a/new.txt", false, 5, 1000), e("same.txt", false, 3, 5000), e("changed.txt", false, 3, 9000), e("tol.txt", false, 3, 7000), e("unknown.txt", false, 3, 0)].into_iter().collect();
    let r: SideMap = [e("same.txt", false, 3, 5000), e("changed.txt", false, 3, 1000), e("tol.txt", false, 3, 5000), e("unknown.txt", false, 3, 123), e("extra", true, 0, 0), e("extra/old.txt", false, 1, 1)]
        .into_iter()
        .collect();
    let p = diff(&m, &r, &spec(false), Detector::SizeMtime, 100_000).unwrap();
    let k = kinds(&p);
    assert_eq!(k[0], ("a".into(), ActionKind::Mkdir, Reason::New)); // parent before child
    assert_eq!(k[1], ("a/new.txt".into(), ActionKind::Copy, Reason::New));
    assert_eq!(k[2], ("changed.txt".into(), ActionKind::Copy, Reason::Changed));
    assert!(k.iter().any(|x| x.0 == "tol.txt" && x.1 == ActionKind::Skip)); // exactly at tolerance is unchanged
    assert!(k.iter().any(|x| x.0 == "unknown.txt" && x.1 == ActionKind::Skip)); // unknown mtime: size only
    assert!(!k.iter().any(|x| x.1 == ActionKind::Delete)); // deletes off
    let p = diff(&m, &r, &spec(true), Detector::SizeMtime, 100_000).unwrap();
    let k = kinds(&p);
    let del: Vec<_> = k.iter().filter(|x| matches!(x.1, ActionKind::Delete | ActionKind::Rmdir)).collect();
    assert_eq!(del[0].0, "extra/old.txt"); // child before parent
    assert_eq!(del[1].0, "extra");
    assert_eq!(p.delete_count(), 2);
    assert!((p.blast_radius_fraction() - 2.0 / 6.0).abs() < 1e-9);
    assert_eq!(p.copy_bytes(), 8);
    // empty master with deletes on is refused
    assert!(diff(&SideMap::new(), &r, &spec(true), Detector::SizeMtime, 0).is_err());
    assert!(diff(&SideMap::new(), &r, &spec(false), Detector::SizeMtime, 0).is_ok());
}

#[test]
fn window_and_offset() {
    let m: SideMap = [e("old.txt", false, 1, 1_000), e("new.txt", false, 1, 90_000), e("ch.txt", false, 2, 95_000)].into_iter().collect();
    let r: SideMap = [e("ch.txt", false, 3, 1), e("gone.txt", false, 1, 1)].into_iter().collect();
    let mut s = spec(true);
    s.modified_within_ms = Some(20_000);
    let p = diff(&m, &r, &s, Detector::SizeMtime, 100_000).unwrap();
    let k = kinds(&p);
    assert!(k.iter().any(|x| x.0 == "old.txt" && x.1 == ActionKind::Skip)); // outside the window: not copied
    assert!(k.iter().any(|x| x.0 == "new.txt" && x.1 == ActionKind::Copy));
    assert!(k.iter().any(|x| x.0 == "ch.txt" && x.1 == ActionKind::Copy));
    assert!(k.iter().any(|x| x.0 == "gone.txt" && x.1 == ActionKind::Delete)); // the window never prevents deletes
                                                                               // offset: median of same-size pairs, needs three
    let m: SideMap = [e("a", false, 1, 10_000), e("b", false, 1, 20_000), e("c", false, 1, 30_000), e("d", false, 9, 99_000)].into_iter().collect();
    let r: SideMap = [e("a", false, 1, 6_400), e("b", false, 1, 16_400), e("c", false, 1, 26_500), e("d", false, 9, 1)].into_iter().collect();
    assert_eq!(auto_offset(&m, &r), 3_600);
    let two: SideMap = m.iter().take(2).map(|(k, v)| (k.to_string(), v.clone())).collect();
    assert_eq!(auto_offset(&two, &r), 0);
    assert!(!is_changed(Detector::SizeMtime, &m["a"], &r["a"], 3_600));
    assert!(is_changed(Detector::SizeMtime, &m["a"], &r["a"], 0));
    assert!(!is_changed(Detector::SizeOnly, &m["a"], &r["a"], 0));
}

#[test]
fn local_end_to_end_is_idempotent() {
    let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join(format!("kiki-mirror-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    // A run writes the audit log, and without this it wrote it in the developer's own
    // `~/.local/state/kiki` — two thousand lines of `mirror-delete keep.txt` by the time it was seen.
    std::env::set_var("KIKI_STATE_DIR", d.join("state"));
    std::fs::create_dir_all(d.join("m/sub")).unwrap();
    std::fs::create_dir_all(d.join("r")).unwrap();
    std::fs::write(d.join("m/a.txt"), b"aaa").unwrap();
    std::fs::write(d.join("m/sub/b.txt"), b"bb").unwrap();
    std::fs::write(d.join("m/sub/c.txt"), b"c").unwrap();
    std::fs::write(d.join("r/keep.txt"), b"k").unwrap();
    let mut s = spec(false);
    s.master = Uri::from_path(&d.join("m"));
    s.replica = Uri::from_path(&d.join("r"));
    let cancel = AtomicBool::new(false);
    let plan = scan(&mut s, &cancel).unwrap();
    assert_eq!(plan.actions.iter().filter(|a| a.kind == ActionKind::Copy).count(), 3);
    assert_eq!(plan.actions.iter().filter(|a| a.kind == ActionKind::Mkdir).count(), 1);
    let plan = Arc::new(Mutex::new(plan));
    let ctx = ExecCtx { cancel: &cancel, workers: 3, on_change: &|_| {}, on_bytes: &|_| {}, exact_times: false };
    let out = execute(&plan, &s, &ctx).unwrap();
    assert_eq!((out.copies, out.deletes, out.skipped), (3, 0, 0));
    assert_eq!(std::fs::read(d.join("r/sub/b.txt")).unwrap(), b"bb");
    assert!(d.join("r/keep.txt").exists()); // additive run keeps replica-only files
                                            // second scan: nothing to do (mtime preserved)
    let plan2 = scan(&mut s, &cancel).unwrap();
    assert!(plan2.actions.iter().all(|a| a.kind == ActionKind::Skip));
    // edit one file: exactly one changed copy
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(d.join("m/a.txt"), b"aaaa").unwrap();
    let plan3 = scan(&mut s, &cancel).unwrap();
    let ch: Vec<_> = plan3.actions.iter().filter(|a| a.kind == ActionKind::Copy).collect();
    assert_eq!(ch.len(), 1);
    assert_eq!(ch[0].reason, Reason::Changed);
    // deletes: blast radius refuses without confirmation, then removes the extra
    s.delete_extras = true;
    let plan4 = Arc::new(Mutex::new(scan(&mut s, &cancel).unwrap()));
    assert_eq!(plan4.lock().unwrap().delete_count(), 1);
    let big = {
        let mut p = plan4.lock().unwrap().clone();
        p.replica_entry_count = 1;
        p
    };
    assert!(execute(&Arc::new(Mutex::new(big)), &s, &ctx).is_err());
    s.confirmed_large_delete = true;
    let out = execute(&plan4, &s, &ctx).unwrap();
    assert_eq!(out.deletes, 1);
    assert!(!d.join("r/keep.txt").exists());
    let text = report(&s, &plan4.lock().unwrap());
    assert!(text.contains("Delete/Extra | keep.txt"));
    std::fs::remove_dir_all(&d).unwrap();
}

// ---------------------------------------------------------------- filters

#[test]
fn filter_rules_match_by_kind() {
    let rules = vec![Rule::Matches(".git".into()), Rule::StartsWith("~".into()), Rule::EndsWith(".tmp".into()), Rule::Contains("cache".into())];
    for name in [".git", "~draft.txt", "build.tmp", "my-cache-dir", "cache"] {
        assert!(filtered(name, &rules), "{name} should be filtered");
    }
    for name in ["git", ".gitignore", "draft~.txt", "tmp.rs", "cach"] {
        assert!(!filtered(name, &rules), "{name} should not be filtered");
    }
    assert!(!filtered("anything", &[]), "no rules filters nothing");
}

/// No `filters.toml` means the defaults, and the file replaces them wholesale.
#[test]
fn filters_come_from_the_config_file_or_the_defaults() {
    let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("kiki-mirror-filters-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("KIKI_CONFIG_DIR", &dir);

    let defaults = load_filters();
    assert!(filtered(".git", &defaults) && filtered(".gitignore", &defaults) && filtered("node_modules", &defaults) && filtered(".DS_Store", &defaults) && filtered(".env", &defaults));
    assert!(!filtered("src", &defaults));
    // A dotted name the defaults do not name is mirrored like anything else: the rules are names
    // now, not the pattern that used to swallow every one of these.
    for name in [".htaccess", ".htpasswd", ".user.ini", ".nojekyll", ".well-known"] {
        assert!(!filtered(name, &defaults), "{name} is not one of the built-in names");
    }

    std::fs::write(dir.join("filters.toml"), "[[rule]]\nkind = \"endsWith\"\nvalue = \".bak\"\n\n[[rule]]\nkind = \"contains\"\nvalue = \"secret\"\n").unwrap();
    let mine = load_filters();
    assert_eq!(mine.len(), 2);
    assert!(filtered("notes.bak", &mine) && filtered("my-secret-file", &mine));
    assert!(!filtered(".git", &mine), "the file replaces the defaults rather than adding to them");

    // A rule missing its kind or value is dropped rather than taken as something else.
    std::fs::write(dir.join("filters.toml"), "[[rule]]\nkind = \"endsWith\"\n\n[[rule]]\nvalue = \"x\"\n").unwrap();
    assert!(load_filters().is_empty());

    std::fs::remove_dir_all(&dir).unwrap();
    std::env::remove_var("KIKI_CONFIG_DIR");
}

/// The "K filtered out" on the Review screen has to be the same number for the same tree however
/// it was read. A local walk stops at a skipped folder and counts it once; a plugin's recursive
/// `Scan` hands back every descendant of it, and counted them all — the same site read "6 filtered
/// out" over SFTP and "1 filtered out" locally. Both sides now ask this.
#[test]
fn a_skipped_subtree_is_counted_once_however_it_was_scanned() {
    let rules = vec![Rule::StartsWith(".".into()), Rule::Matches("node_modules".into())];
    assert_eq!(filtered_rel("visible.txt", &rules), None);
    assert_eq!(filtered_rel("src/main.rs", &rules), None);
    assert_eq!(filtered_rel(".hidden", &rules), Some(true));
    assert_eq!(filtered_rel(".git", &rules), Some(true), "the folder itself is the one counted");
    assert_eq!(filtered_rel(".git/HEAD", &rules), Some(false), "and nothing under it is counted again");
    assert_eq!(filtered_rel(".git/refs/heads/main", &rules), Some(false));
    assert_eq!(filtered_rel("src/node_modules", &rules), Some(true), "wherever it is in the tree");
    assert_eq!(filtered_rel("src/node_modules/left-pad/index.js", &rules), Some(false));
    // A whole recursive answer, as a plugin streams it: three names skipped, not eight.
    let deep = [".hidden", ".git", ".git/HEAD", ".git/refs", ".git/refs/heads/main", "node_modules", "node_modules/left-pad/index.js", "src/main.rs", "visible.txt"];
    assert_eq!(deep.iter().filter(|r| filtered_rel(r, &rules) == Some(true)).count(), 3);
    assert_eq!(deep.iter().filter(|r| filtered_rel(r, &rules).is_none()).count(), 2, "and two names survive");
}

/// What the Edit rules… dialog reads and writes: `MirrorFilters` and `SetMirrorFilters`, which
/// are these four functions. The one that matters is the empty list — an empty FILE means the
/// defaults, so "the user deleted every rule" has to be written down as something else, or the
/// next scan quietly brings `.git` and the dotfiles back.
#[test]
fn the_rules_round_trip_and_no_rules_is_not_the_same_as_the_defaults() {
    let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("kiki-mirror-rules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("KIKI_CONFIG_DIR", &dir);
    let file = dir.join("filters.toml");
    let kinds = |v: &Value| v.get("rules").and_then(Value::as_arr).unwrap().iter().map(|r| format!("{} {}", r.str_field("kind").unwrap(), r.str_field("value").unwrap())).collect::<Vec<_>>();

    // The built-in set, in the words the reply uses, so the list below is the list on screen.
    let built_in = ["matches .git", "matches .gitignore", "matches .DS_Store", "matches .env", "matches .idea", "matches .vscode", "matches Thumbs.db", "matches node_modules", "matches __pycache__"];
    // What the dialog's "Restore defaults" shows: the daemon's own list, sent with every answer
    // rather than kept a second time in the shell.
    let default_kinds = |v: &Value| v.get("defaultRules").and_then(Value::as_arr).unwrap().iter().map(|r| format!("{} {}", r.str_field("kind").unwrap(), r.str_field("value").unwrap())).collect::<Vec<_>>();

    // Nothing written down: the defaults, and the answer says they are the defaults.
    let (rules, defaults) = filters();
    assert_eq!(rules, default_rules());
    assert!(defaults);
    assert_eq!(kinds(&filters_json()), built_in);
    assert_eq!(default_kinds(&filters_json()), built_in);
    assert_eq!(filters_json().get("defaults").and_then(Value::as_bool), Some(true));

    // The user's own rules go out and come back the same, no longer the defaults.
    let mine = vec![Rule::EndsWith(".tmp".into()), Rule::Contains("draft".into()), Rule::StartsWith(".".into()), Rule::Matches("target".into())];
    set_filters(&mine).unwrap();
    assert_eq!(filters(), (mine.clone(), false));
    assert_eq!(kinds(&filters_json()), ["endsWith .tmp", "contains draft", "startsWith .", "matches target"]);
    assert_eq!(filters_json().get("defaults").and_then(Value::as_bool), Some(false));
    assert_eq!(default_kinds(&filters_json()), built_in, "the built-in set goes out whether or not it is the one in use");
    assert!(filtered("notes.tmp", &filters().0) && !filtered("notes.txt", &filters().0));

    // Every rule deleted is not "no file": it is no rules at all, and it survives the next read.
    set_filters(&[]).unwrap();
    assert_eq!(filters(), (Vec::new(), false));
    assert!(std::fs::read_to_string(&file).unwrap().contains("defaults = false"));
    assert!(!filtered(".git", &filters().0), "with the rules deleted, nothing is skipped");
    assert_eq!(default_kinds(&filters_json()), built_in, "and the dialog can still offer to put them back");

    // Restore defaults takes the file away, which is what puts the built-in rules back. Doing it
    // twice is not an error — there is nothing left to remove the second time.
    restore_default_filters().unwrap();
    assert!(!file.exists());
    assert_eq!(filters(), (default_rules(), true));
    restore_default_filters().unwrap();

    std::fs::remove_dir_all(&dir).unwrap();
    std::env::remove_var("KIKI_CONFIG_DIR");
}

/// A rule the shell sends is checked before any of it is written, so a refusal leaves the file
/// exactly as it was rather than half saved.
#[test]
fn a_rule_that_could_never_match_is_refused_by_name() {
    let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("kiki-mirror-badrules-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("KIKI_CONFIG_DIR", &dir);
    let rule = |kind: &str, value: &str| Value::obj().s("kind", kind).s("value", value).done();

    let good = vec![rule("matches", ".git"), rule("startsWith", "."), rule("endsWith", ".tmp"), rule("contains", "cache")];
    assert_eq!(check_rules(&good).unwrap().len(), 4);
    set_filters(&check_rules(&good).unwrap()).unwrap();
    let before = std::fs::read_to_string(dir.join("filters.toml")).unwrap();

    for (rules, says) in [
        (vec![rule("matches", ".git"), rule("regex", "^\\.")], "rule 2: kind must be contains, startsWith, endsWith or matches"),
        (vec![rule("matches", "")], "rule 1: value must not be empty"),
        (vec![rule("matches", ".git"), rule("matches", "x"), rule("endsWith", "build/out")], "rule 3: value must not contain a slash — a rule matches a name, not a path"),
    ] {
        assert_eq!(check_rules(&rules).unwrap_err(), says);
    }
    // A missing field is the same refusal as a wrong one, not a rule that quietly becomes `matches`.
    assert_eq!(check_rules(&[Value::obj().s("value", "x").done()]).unwrap_err(), "rule 1: kind must be contains, startsWith, endsWith or matches");
    assert_eq!(check_rules(&[rule("matches", ".git"), Value::obj().s("kind", "contains").done()]).unwrap_err(), "rule 2: value must not be empty");
    assert_eq!(std::fs::read_to_string(dir.join("filters.toml")).unwrap(), before, "a refused save leaves the file alone");

    std::fs::remove_dir_all(&dir).unwrap();
    std::env::remove_var("KIKI_CONFIG_DIR");
}

/// The default rules against a real tree: each of the eight names goes, a named FOLDER takes its
/// whole subtree with it, and every other name is mirrored — dotted or not. The dotted ones here
/// are the reason the defaults are names rather than `startsWith "."`: a website that loses its
/// `.htaccess` and its `.well-known/acme-challenge/` stops redirecting and stops renewing its
/// certificate, and the only sign of it is "N filtered out".
#[test]
fn the_default_rules_skip_the_eight_names_and_mirror_the_rest() {
    let cancel = AtomicBool::new(false);
    let d = std::env::temp_dir().join(format!("kiki-mirror-hidden-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    for dir in [".git/refs", ".idea", ".vscode", "node_modules/left-pad", "__pycache__", ".well-known/acme-challenge", "src"] {
        std::fs::create_dir_all(d.join(dir)).unwrap();
    }
    // Skipped, every one of them named in `default_rules`.
    std::fs::write(d.join(".git/HEAD"), b"ref").unwrap();
    std::fs::write(d.join(".git/refs/main"), b"sha").unwrap();
    std::fs::write(d.join(".env"), b"SECRET=1").unwrap();
    std::fs::write(d.join(".DS_Store"), b"\0").unwrap();
    std::fs::write(d.join(".idea/workspace.xml"), b"<x/>").unwrap();
    std::fs::write(d.join(".vscode/settings.json"), b"{}").unwrap();
    std::fs::write(d.join("Thumbs.db"), b"\0").unwrap();
    std::fs::write(d.join("node_modules/left-pad/index.js"), b"//").unwrap();
    std::fs::write(d.join("__pycache__/mod.pyc"), b"\0").unwrap();
    // Mirrored: the four dotted names a server needs, the challenge directory, and the ordinary.
    std::fs::write(d.join(".htaccess"), b"Redirect /").unwrap();
    std::fs::write(d.join(".user.ini"), b"x=1").unwrap();
    std::fs::write(d.join(".nojekyll"), b"").unwrap();
    std::fs::write(d.join(".well-known/acme-challenge/token"), b"tok").unwrap();
    std::fs::write(d.join("visible.txt"), b"v").unwrap();
    std::fs::write(d.join("src/main.rs"), b"fn main(){}").unwrap();

    let side = side_for(&Uri::from_path(&d)).unwrap();
    let mut n = 0;
    let map = scan_side(&side, &default_rules(), &mut n, &cancel).unwrap();
    let mut got: Vec<&str> = map.iter().map(|(rel, _)| rel).collect();
    got.sort();
    assert_eq!(got, [".htaccess", ".nojekyll", ".user.ini", ".well-known", ".well-known/acme-challenge", ".well-known/acme-challenge/token", "src", "src/main.rs", "visible.txt"]);
    assert_eq!(n, 8, "the eight names — a skipped folder is counted once, not per file");

    // And the pattern that used to be the default is still there for whoever wants it: adding it
    // by hand takes the same dotted names away again.
    let mut hidden_too = default_rules();
    hidden_too.push(Rule::StartsWith(".".into()));
    let mut n2 = 0;
    let map2 = scan_side(&side, &hidden_too, &mut n2, &cancel).unwrap();
    let mut got2: Vec<&str> = map2.iter().map(|(rel, _)| rel).collect();
    got2.sort();
    assert_eq!(got2, ["src", "src/main.rs", "visible.txt"]);

    let _ = std::fs::remove_dir_all(&d);
}

/// The guard that matters on the destination: a name the rules skip is not an "extra", so it is
/// never planned for deletion however emphatically deletes are on.
#[test]
fn a_filtered_name_on_the_destination_is_never_an_extra() {
    let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join(format!("kiki-mirror-extra-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::env::set_var("KIKI_CONFIG_DIR", d.join("config"));
    std::fs::create_dir_all(d.join("config")).unwrap();
    std::fs::create_dir_all(d.join("r/node_modules")).unwrap();
    std::fs::create_dir_all(d.join("m")).unwrap();
    std::fs::write(d.join("m/page.html"), b"<p>").unwrap();
    std::fs::write(d.join("r/page.html"), b"<p>").unwrap();
    std::fs::write(d.join("r/.env"), b"SECRET=1").unwrap();
    std::fs::write(d.join("r/node_modules/left.js"), b"//").unwrap();

    let mut s = spec(true);
    s.apply_filters = true;
    s.master = Uri::from_path(&d.join("m"));
    s.replica = Uri::from_path(&d.join("r"));
    let cancel = AtomicBool::new(false);
    let plan = scan(&mut s, &cancel).unwrap();
    assert_eq!(plan.delete_count(), 0, "{:?}", kinds(&plan));
    assert_eq!(plan.filtered_count, 2, ".env and node_modules");
    assert_eq!(plan.replica_entry_count, 1, "and they are not counted as items on the destination either");

    // With the rules off, they are extras like anything else — the guard is the rules, not luck.
    s.apply_filters = false;
    let plan = scan(&mut s, &cancel).unwrap();
    assert_eq!(plan.delete_count(), 3);

    std::env::remove_var("KIKI_CONFIG_DIR");
    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- detectors

#[test]
fn detectors_decide_what_counts_as_changed() {
    let f = |size: u64, mtime: u64, digest: Option<&str>| Entry { path: 0, is_dir: false, size, mtime_ms: mtime, digest: digest.map(str::to_string) };
    let a = f(10, 1_000_000, Some("0123456789abcdef0123456789abcdef"));
    let same_size_other_time = f(10, 1_009_000, Some("0123456789abcdef0123456789abcdef"));
    let other_size = f(11, 1_000_000, Some("0123456789abcdef0123456789abcdef"));
    let other_digest = f(10, 1_000_000, Some("ffffffffffffffffffffffffffffffff"));

    // size only ignores the clock entirely
    assert!(!is_changed(Detector::SizeOnly, &a, &same_size_other_time, 0));
    assert!(is_changed(Detector::SizeOnly, &a, &other_size, 0));

    // size+mtime: a difference beyond the tolerance counts, and the offset cancels it. The offset
    // is *subtracted* from the master's mtime, so a master that reads 9 s earlier than the replica
    // is reconciled by -9000, not by 9000.
    assert!(is_changed(Detector::SizeMtime, &a, &same_size_other_time, 0));
    assert!(!is_changed(Detector::SizeMtime, &a, &same_size_other_time, -9_000));
    assert!(is_changed(Detector::SizeMtime, &a, &same_size_other_time, 9_000), "the wrong sign makes it worse");
    assert!(!is_changed(Detector::SizeMtime, &a, &f(10, 1_000_000 + TOLERANCE_MS as u64, None), 0), "exactly at the tolerance is unchanged");

    // an unknown mtime on either side falls back to size alone
    assert!(!is_changed(Detector::SizeMtime, &f(10, 0, None), &same_size_other_time, 0));
    assert!(is_changed(Detector::SizeMtime, &f(11, 0, None), &same_size_other_time, 0));

    // digests: same size and same digest is unchanged; a different digest is not
    assert!(!is_changed(Detector::Digest, &a, &f(10, 9_999_999, Some("0123456789ABCDEF0123456789ABCDEF")), 0), "case does not matter");
    assert!(is_changed(Detector::Digest, &a, &other_digest, 0));
    assert!(is_changed(Detector::Digest, &a, &other_size, 0));
    // a digest only one side has, or one that is not an md5, falls back to size+mtime
    assert!(is_changed(Detector::Digest, &a, &f(10, 1_009_000, None), 0));
    assert!(is_changed(Detector::Digest, &a, &f(10, 1_009_000, Some("not-a-digest")), 0));
}

#[test]
fn the_clock_offset_is_the_median_of_matching_pairs() {
    let pair = |rel: &str, mtime_m: u64, mtime_r: u64| (e(rel, false, 4, mtime_m), e(rel, false, 4, mtime_r));
    let mut m = SideMap::new();
    let mut r = SideMap::new();
    for (rel, dm, dr) in [("a", 10_000u64, 5_000u64), ("b", 20_000, 13_000), ("c", 30_000, 25_000)] {
        let (mm, rr) = pair(rel, dm, dr);
        m.insert(&mm.0, mm.1);
        r.insert(&rr.0, rr.1);
    }
    assert_eq!(auto_offset(&m, &r), 5_000, "deltas 5000, 7000, 5000 → median 5000");

    // Fewer than three usable pairs is not enough to conclude anything.
    let mut few = SideMap::new();
    few.insert("a", e("a", false, 4, 10_000).1);
    assert_eq!(auto_offset(&few, &r), 0);

    // Deltas that do not agree are not a clock (2026-09-24): every local file re-stamped by a
    // checkout at one moment, the replica written over months — the median was days, and
    // meaningless. Half or fewer within the tolerance of the median: no offset.
    let mut stamped = SideMap::new();
    let mut written = SideMap::new();
    for (rel, dr) in [("a", 1_000u64), ("b", 700_000_000), ("c", 40_000_000), ("d", 300_000_000), ("e", 1_200_000_000)] {
        stamped.insert(rel, e(rel, false, 4, 1_500_000_000).1);
        written.insert(rel, e(rel, false, 4, dr).1);
    }
    assert_eq!(auto_offset(&stamped, &written), 0, "a checkout's stamps are not a clock");
    // Two of three agreeing (5000, 7000, 5000) is still a clock, as above; a majority is enough.

    // Directories, differing sizes and unknown mtimes are not samples.
    let mut dirs = SideMap::new();
    let mut dirs_r = SideMap::new();
    for rel in ["x", "y", "z"] {
        dirs.insert(rel, e(rel, true, 0, 10_000).1);
        dirs_r.insert(rel, e(rel, true, 0, 1_000).1);
    }
    assert_eq!(auto_offset(&dirs, &dirs_r), 0);
}

/// The manual offset, end to end through `scan` — the path the Configure screen's hours box
/// drives. The comparison is `master.mtime − offset − replica.mtime`, so the offset is what the
/// SOURCE reads more than the destination for one and the same file: a destination whose clock is
/// three hours behind is reconciled by **+3 h**, and by −3 h it is twice as wrong. A sign the
/// wrong way about copies the whole tree on every run, or skips all of it; hence both directions
/// here, and the wrong sign asserted as well as the right one.
#[test]
fn a_manual_clock_offset_is_used_as_given_and_auto_measures_the_same_one() {
    const HOUR: i64 = 3_600_000;
    let d = std::env::temp_dir().join(format!("kiki-mirror-offset-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("m")).unwrap();
    std::fs::create_dir_all(d.join("r")).unwrap();
    // The same four files on both sides, byte for byte: only the clocks disagree.
    let stamp = |side: &str, n: i64, ms: i64| {
        let p = d.join(side).join(format!("f{n}.txt"));
        std::fs::write(&p, format!("file {n}")).unwrap();
        crate::ops::set_mtime(&p, std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64)).unwrap();
    };
    let base: i64 = 1_700_000_000_000;
    let both = |replica_shift: i64| {
        for n in 0..4 {
            stamp("m", n, base + n * 1_000);
            stamp("r", n, base + n * 1_000 + replica_shift);
        }
    };
    let cancel = AtomicBool::new(false);
    let mut s = spec(false);
    s.master = Uri::from_path(&d.join("m"));
    s.replica = Uri::from_path(&d.join("r"));
    s.clock_offset_auto = false;
    let copies = |s: &mut Spec| {
        let p = scan(s, &cancel).unwrap();
        assert_eq!(p.clock_offset_ms, s.clock_offset_ms, "the plan carries the offset it was diffed with — the report prints this one");
        p.actions.iter().filter(|a| a.kind == ActionKind::Copy).count()
    };

    // A destination three hours BEHIND the source: +3 h is the offset that reconciles it.
    both(-3 * HOUR);
    s.clock_offset_ms = 0;
    assert_eq!(copies(&mut s), 4, "with no offset every file looks three hours out of date");
    s.clock_offset_ms = 3 * HOUR;
    assert_eq!(copies(&mut s), 0, "with the offset given by hand there is nothing to do");
    s.clock_offset_ms = -3 * HOUR;
    assert_eq!(copies(&mut s), 4, "the wrong sign is six hours out, not none");
    s.clock_offset_auto = true;
    s.clock_offset_ms = 0;
    assert_eq!(copies(&mut s), 0, "measured for itself, the same answer as the hand-set one");
    assert_eq!(s.clock_offset_ms, 3 * HOUR, "and the scan writes what it measured back into the spec");

    // And the other way about: a destination three hours AHEAD is −3 h.
    both(3 * HOUR);
    s.clock_offset_auto = false;
    s.clock_offset_ms = -3 * HOUR;
    assert_eq!(copies(&mut s), 0);
    s.clock_offset_ms = 3 * HOUR;
    assert_eq!(copies(&mut s), 4, "the wrong sign again");
    s.clock_offset_auto = true;
    assert_eq!(copies(&mut s), 0);
    assert_eq!(s.clock_offset_ms, -3 * HOUR);

    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_local_only_mirror_uses_size_and_mtime() {
    let mut s = spec(false);
    s.detector = Detector::Auto;
    assert_eq!(pick_detector(&s), Detector::SizeMtime, "nothing remote to ask");
    s.detector = Detector::Digest;
    assert_eq!(pick_detector(&s), Detector::Digest, "an explicit choice is kept");
}

// ---------------------------------------------------------------- the plan registry

/// The plan store is one map for the whole process: tests that count what is in it take turns.
static PLANS: Mutex<()> = Mutex::new(());

fn one_file_plan() -> Plan {
    let m: SideMap = [e("a.txt", false, 1, 1)].into_iter().collect();
    diff(&m, &SideMap::new(), &spec(false), Detector::SizeMtime, 0).unwrap()
}

#[test]
fn a_plan_goes_when_the_last_view_of_it_closes() {
    let _g = PLANS.lock().unwrap_or_else(|e| e.into_inner());
    store(7001, spec(false), one_file_plan());
    assert!(view(7001).is_some());
    assert!(view(7001).is_some(), "two windows showing it");
    unview(7001);
    assert!(stored(7001).is_some(), "one is still looking");
    unview(7001);
    assert!(stored(7001).is_none(), "a plan lists every file on both sides: not kept for nobody");
    unview(7001); // a second close of the same view is not an error
    assert!(view(7002).is_none(), "no such plan");
}

#[test]
fn a_finished_run_drops_a_plan_nobody_is_showing() {
    let _g = PLANS.lock().unwrap_or_else(|e| e.into_inner());
    store(7010, spec(false), one_file_plan());
    view(7010);
    run_finished(7010);
    assert!(stored(7010).is_some(), "still on screen: the report can be saved");
    unview(7010);
    store(7011, spec(false), one_file_plan());
    let held = stored(7011).unwrap(); // what a run holds while it works
    run_finished(7011);
    assert!(stored(7011).is_none());
    assert_eq!(held.plan.lock().unwrap().actions.len(), 1, "the run's own handle outlives the store's");
}

#[test]
fn plans_nobody_opened_are_capped_and_the_ones_on_screen_survive() {
    let _g = PLANS.lock().unwrap_or_else(|e| e.into_inner());
    store(7100, spec(false), one_file_plan());
    view(7100); // the oldest, but on screen
    for id in 7101..7101 + KEEP as u64 + 4 {
        store(id, spec(false), one_file_plan());
    }
    let mine: Vec<u64> = store::ids().into_iter().filter(|id| (7100..7200).contains(id)).collect();
    assert!(mine.len() <= KEEP, "{mine:?}");
    assert!(mine.contains(&7100), "never the one being shown");
    assert!(mine.contains(&(7100 + KEEP as u64 + 4)), "nor the newest");
    assert!(!mine.contains(&7101), "the oldest idle one went first");
    unview(7100);
    for id in mine {
        run_finished(id);
    }
}

#[test]
fn a_scanned_plan_is_kept_for_the_run_that_follows() {
    let _g = PLANS.lock().unwrap_or_else(|e| e.into_inner());
    let m: SideMap = [e("a.txt", false, 1, 1)].into_iter().collect();
    let plan = diff(&m, &SideMap::new(), &spec(false), Detector::SizeMtime, 0).unwrap();
    assert!(stored(4242).is_none(), "an unknown job id has no plan");
    store(4242, spec(false), plan);
    let got = stored(4242).expect("the plan the user reviewed");
    assert_eq!(got.plan.lock().unwrap().actions.len(), 1);
    assert_eq!(got.spec.master.to_string(), "file:///m");
    assert!(Arc::ptr_eq(&got.plan, &stored(4242).unwrap().plan), "the same plan, not a copy");
}

// ---------------------------------------------------------------- guards

/// The rails that stand between a mirror and someone's files. Every one of them is a refusal, so
/// each is worth a test of its own: a plan that deletes more than the user agreed to, and a plan
/// carrying a path that would reach outside the replica.
#[test]
fn the_guards_refuse_before_anything_is_deleted() {
    let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cancel = AtomicBool::new(false);
    let ctx = ExecCtx { cancel: &cancel, workers: 1, on_change: &|_| {}, on_bytes: &|_| {}, exact_times: false };
    let d = std::env::temp_dir().join(format!("kiki-mirror-guards-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::env::set_var("KIKI_STATE_DIR", d.join("state")); // the audit log: see above
    std::fs::create_dir_all(d.join("r")).unwrap();
    for n in ["one", "two", "three", "four"] {
        std::fs::write(d.join("r").join(n), b"x").unwrap();
    }
    let mut s = spec(true);
    s.master = Uri::from_path(&d.join("m"));
    s.replica = Uri::from_path(&d.join("r"));
    s.blast_radius = 0.5;

    // Three of four deleted is past half: refused, and the files are still there.
    let m: SideMap = [e("one", false, 1, 1)].into_iter().collect();
    let r: SideMap = [e("one", false, 1, 1), e("two", false, 1, 1), e("three", false, 1, 1), e("four", false, 1, 1)].into_iter().collect();
    let plan = diff(&m, &r, &s, Detector::SizeOnly, 0).unwrap();
    assert_eq!(plan.delete_count(), 3);
    let err = execute(&Arc::new(Mutex::new(plan)), &s, &ctx).err().expect("a plan past the blast radius is refused");
    assert!(err.message().contains("Safety"), "{}", err.message());
    assert!(d.join("r/two").exists(), "nothing was deleted");

    // Confirmed, the same plan runs.
    s.confirmed_large_delete = true;
    let plan = diff(&m, &r, &s, Detector::SizeOnly, 0).unwrap();
    let out = execute(&Arc::new(Mutex::new(plan)), &s, &ctx).expect("confirmed, the same plan runs");
    assert_eq!(out.deletes, 3);
    assert!(!d.join("r/two").exists() && d.join("r/one").exists());

    // A relative path that climbs out of the replica is refused whatever the user confirmed.
    let mut plan = diff(&m, &r, &s, Detector::SizeOnly, 0).unwrap();
    plan.actions.push(Action {
        rel: "../outside".into(),
        kind: ActionKind::Delete,
        reason: Reason::Extra,
        master: None,
        replica: Some(e("x", false, 1, 1).1),
        bytes: 0,
        checked: true,
        state: State::Pending,
        progress: 0,
        error: None,
    });
    let err = execute(&Arc::new(Mutex::new(plan)), &s, &ctx).err().expect("a path climbing out of the replica is refused");
    assert!(err.message().contains("outside the replica root"), "{}", err.message());

    // And so is an absolute one.
    let mut plan = diff(&m, &r, &s, Detector::SizeOnly, 0).unwrap();
    plan.actions
        .push(Action { rel: "/etc/passwd".into(), kind: ActionKind::Delete, reason: Reason::Extra, master: None, replica: None, bytes: 0, checked: true, state: State::Pending, progress: 0, error: None });
    assert!(execute(&Arc::new(Mutex::new(plan)), &s, &ctx).err().expect("an absolute path is refused").message().contains("outside the replica root"));

    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- scanning

#[test]
fn a_local_scan_walks_into_folders_and_skips_what_the_filters_name() {
    let cancel = AtomicBool::new(false);
    let d = std::env::temp_dir().join(format!("kiki-mirror-scan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("sub/deeper")).unwrap();
    std::fs::create_dir_all(d.join(".git")).unwrap();
    std::fs::write(d.join("a.txt"), b"aaa").unwrap();
    std::fs::write(d.join("sub/b.txt"), b"bb").unwrap();
    std::fs::write(d.join("sub/deeper/c.txt"), b"c").unwrap();
    std::fs::write(d.join(".git/config"), b"x").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(d.join("a.txt"), d.join("link.txt")).unwrap();

    let side = side_for(&Uri::from_path(&d)).unwrap();
    let mut filtered_count = 0;
    let map = scan_side(&side, &[Rule::Matches(".git".into())], &mut filtered_count, &cancel).unwrap();

    assert!(map.get("a.txt").is_some() && map.get("sub/b.txt").is_some() && map.get("sub/deeper/c.txt").is_some());
    assert_eq!(map.get("a.txt").unwrap().size, 3);
    assert!(map.get("sub").unwrap().is_dir);
    assert_eq!(map.get("sub").unwrap().size, 0, "a folder carries no size");
    assert!(map.get(".git").is_none() && map.get(".git/config").is_none(), "a filtered folder is not descended into");
    assert_eq!(filtered_count, 1);
    #[cfg(unix)]
    assert!(map.get("link.txt").is_none(), "symlinks are not mirrored");
    assert!(map.get("a.txt").unwrap().mtime_ms > 0);

    // Cancelling stops the walk with an error rather than a partial map.
    let stop = AtomicBool::new(true);
    let mut n = 0;
    assert!(scan_side(&side, &[], &mut n, &stop).is_err());

    let _ = std::fs::remove_dir_all(&d);
}

// ---------------------------------------------------------------- report

#[test]
fn the_report_says_what_the_run_would_do() {
    let m: SideMap = [e("new.txt", false, 5, 1_700_000_000_000), e("gone", true, 0, 0)].into_iter().collect();
    let r: SideMap = [e("extra.txt", false, 9, 1_700_000_000_000)].into_iter().collect();
    let mut s = spec(true);
    s.confirmed_large_delete = true;
    s.modified_within_ms = Some(7_200_000);
    let plan = diff(&m, &r, &s, Detector::SizeMtime, 1_700_000_100_000).unwrap();

    let text = report(&s, &plan);
    assert!(text.starts_with("kiki mirror report"));
    assert!(text.contains("upload (local → remote)"));
    assert!(text.contains("Detector:          size+mtime"));
    assert!(text.contains("Modified within:   2 h"));
    assert!(text.contains("to copy,"));
    assert!(text.contains("new.txt"), "every action is listed");
    assert!(!text.contains("Skip/"), "skips are left out");

    // The same plan as JSON, which is what the window draws.
    let j = action_json(plan.actions.iter().find(|a| &*a.rel == "new.txt").unwrap());
    assert_eq!(j.str_field("rel"), Some("new.txt"));
    assert_eq!(j.str_field("action"), Some("copy"));
    assert_eq!(j.str_field("reason"), Some("new"));
    assert_eq!(j.u64_field("bytes"), Some(5));
    assert_eq!(j.get("master").unwrap().u64_field("size"), Some(5));
    assert!(matches!(j.get("replica"), Some(Value::Null)), "no replica side for a new file");
}

/// What the workspace refuses, the daemon refuses too: a client that skips the form cannot mirror
/// a folder into itself.
#[test]
fn a_folder_is_not_mirrored_into_itself() {
    let u = |s: &str| Uri::parse(s).unwrap();
    let cancel = AtomicBool::new(false);
    for (m, r) in [("file:///a/b", "file:///a/b"), ("file:///a/b", "file:///a/b/c"), ("file:///a/b/c", "file:///a/b"), ("file:///", "file:///a"), ("sftp://lab/srv", "sftp://lab/srv/site")] {
        assert!(scan::overlap(&u(m), &u(r)).is_err(), "{m} → {r}");
    }
    // A name that only begins the same is another folder; so is the same path on another machine.
    for (m, r) in [("file:///a/b", "file:///a/bc"), ("file:///a/b", "sftp://lab/a/b"), ("sftp://lab/srv", "sftp://nas/srv/site")] {
        assert!(scan::overlap(&u(m), &u(r)).is_ok(), "{m} → {r}");
    }
    // And it is asked before either side is touched: neither of these folders exists.
    let mut s = spec(true);
    s.master = u("file:///kiki-no-such/a");
    s.replica = u("file:///kiki-no-such/a/b");
    let err = format!("{:?}", scan(&mut s, &cancel).err());
    assert!(err.contains("inside the source"), "{err}");
}

// ---------------------------------------------------------------- cancelling a compare

/// A compare of a big tree, stopped half way: it must come back at once, say it was cancelled,
/// and stop WHERE IT WAS rather than walking the rest of the tree "just to finish". The count it
/// reports as it goes is both what the Preflight screen shows and what says where it stopped.
#[test]
fn a_big_local_compare_stops_where_it_was_cancelled() {
    let _guard = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = std::env::temp_dir().join(format!("kiki-mirror-cancel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::env::set_var("KIKI_STATE_DIR", d.join("state"));
    let entries: u64 = 10_000;
    for dir in 0..100 {
        let p = d.join("m").join(format!("d{dir:03}"));
        std::fs::create_dir_all(&p).unwrap();
        for f in 0..(entries / 100 - 1) {
            std::fs::write(p.join(format!("f{f:04}.txt")), b"x").unwrap();
        }
    }
    std::fs::create_dir_all(d.join("r")).unwrap();
    let mut s = spec(false);
    s.master = Uri::from_path(&d.join("m"));
    s.replica = Uri::from_path(&d.join("r"));

    // Cancelled the moment the walk first says where it has got to — which is a few hundred
    // entries in, wherever this machine happens to be by then.
    let cancel = AtomicBool::new(false);
    let seen = std::sync::atomic::AtomicU64::new(0);
    let at = Mutex::new(None);
    let started = std::time::Instant::now();
    let out = scan_counting(&mut s, &cancel, &|n| {
        seen.store(n, Ordering::Relaxed);
        if !cancel.swap(true, Ordering::Relaxed) {
            *at.lock().unwrap() = Some(std::time::Instant::now());
        }
    });
    let at = at.lock().unwrap().expect("the walk said where it had got to");
    assert!(out.is_err(), "a compare that was cancelled has no plan to show (it ran to the end in {:?})", started.elapsed());
    assert_eq!(out.err().map(|e| e.message()), Some("cancelled".to_string()));
    assert!(at.elapsed() < std::time::Duration::from_millis(500), "it took {:?} to stop", at.elapsed());
    let stopped_at = seen.load(Ordering::Relaxed);
    assert!(stopped_at < entries / 4, "it walked on to {stopped_at} of {entries} entries after being cancelled");
    std::fs::remove_dir_all(&d).unwrap();
}

/// The Digest detector hashes whole files on the local side. A cancel is looked at INSIDE each
/// file, not only between them — a mirror of disc images would otherwise ignore Cancel until the
/// one being hashed was done.
#[test]
fn a_cancelled_compare_gives_up_on_the_file_it_is_hashing() {
    let d = std::env::temp_dir().join(format!("kiki-mirror-digest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("a.bin"), vec![b'a'; 4096]).unwrap();
    let side = Side::Local(d.clone());
    // The other side is a backend that hands out content hashes (an object store's ETag).
    let other: SideMap = [("a.bin".to_string(), Entry { path: 0, is_dir: false, size: 4096, mtime_ms: 0, digest: Some(crate::md5::hex(&vec![b'a'; 4096])) })].into_iter().collect();

    let mut mine: SideMap = [e("a.bin", false, 4096, 0)].into_iter().collect();
    let cancel = AtomicBool::new(false);
    scan::fill_local_digests(&side, &mut mine, &other, &cancel);
    assert_eq!(mine.get("a.bin").unwrap().digest, other.get("a.bin").unwrap().digest, "the local side is hashed to compare with the digest");
    assert!(!is_changed(Detector::Digest, mine.get("a.bin").unwrap(), other.get("a.bin").unwrap(), 0));

    let mut mine: SideMap = [e("a.bin", false, 4096, 0)].into_iter().collect();
    cancel.store(true, Ordering::Relaxed);
    scan::fill_local_digests(&side, &mut mine, &other, &cancel);
    assert_eq!(mine.get("a.bin").unwrap().digest, None, "a cancelled compare hashes nothing");
    std::fs::remove_dir_all(&d).unwrap();
}
