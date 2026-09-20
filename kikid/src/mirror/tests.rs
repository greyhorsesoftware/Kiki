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
    let ctx = ExecCtx { cancel: &cancel, workers: 3, on_change: &|_| {}, on_bytes: &|_| {} };
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
    assert!(filtered(".git", &defaults) && filtered("node_modules", &defaults) && filtered(".DS_Store", &defaults));
    assert!(!filtered("src", &defaults));

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

    // Directories, differing sizes and unknown mtimes are not samples.
    let mut dirs = SideMap::new();
    let mut dirs_r = SideMap::new();
    for rel in ["x", "y", "z"] {
        dirs.insert(rel, e(rel, true, 0, 10_000).1);
        dirs_r.insert(rel, e(rel, true, 0, 1_000).1);
    }
    assert_eq!(auto_offset(&dirs, &dirs_r), 0);
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
    let cancel = AtomicBool::new(false);
    let ctx = ExecCtx { cancel: &cancel, workers: 1, on_change: &|_| {}, on_bytes: &|_| {} };
    let d = std::env::temp_dir().join(format!("kiki-mirror-guards-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
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
    plan.actions.push(Action { rel: "../outside".into(), kind: ActionKind::Delete, reason: Reason::Extra, master: None, replica: Some(e("x", false, 1, 1).1), bytes: 0, checked: true, state: State::Pending, progress: 0, error: None });
    let err = execute(&Arc::new(Mutex::new(plan)), &s, &ctx).err().expect("a path climbing out of the replica is refused");
    assert!(err.message().contains("outside the replica root"), "{}", err.message());

    // And so is an absolute one.
    let mut plan = diff(&m, &r, &s, Detector::SizeOnly, 0).unwrap();
    plan.actions.push(Action { rel: "/etc/passwd".into(), kind: ActionKind::Delete, reason: Reason::Extra, master: None, replica: None, bytes: 0, checked: true, state: State::Pending, progress: 0, error: None });
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
