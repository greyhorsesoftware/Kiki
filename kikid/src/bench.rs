//! Benchmarks (plan 26): synthetic trees, one timed pass over the daemon's hot paths, JSON
//! results, and a comparison that fails on regressions.
//!
//! `kikid bench gen <profile> <dir>`   flat10k | flat200k | deep100k | photos | all
//! `kikid bench run <dir> [--json out]` measure every profile found under <dir> (or one directory)
//! `kikid bench compare <baseline.json> <results.json> [--tolerance 25]`

use crate::json::Value;
use crate::listing::{self, SortRole, Subscriber};
use crate::vfs::uri::Uri;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const PROFILES: [&str; 4] = ["flat10k", "flat200k", "deep100k", "photos"];

// ---------------------------------------------------------------- synthetic trees

pub fn gen(profile: &str, dir: &Path) -> std::io::Result<()> {
    match profile {
        "all" => {
            for p in PROFILES {
                gen(p, &dir.join(p))?;
            }
            Ok(())
        }
        "flat10k" => flat(dir, 10_000),
        "flat200k" => flat(dir, 200_000),
        "deep100k" => deep(dir),
        "photos" => photos(dir, 200),
        other => Err(std::io::Error::other(format!("unknown profile {other}"))),
    }
}

fn flat(dir: &Path, n: usize) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for i in 0..n {
        let name = match i % 7 {
            0 => format!("Report {i}.pdf"),
            1 => format!("photo_{i:06}.jpg"),
            2 => format!("notes-{i}.md"),
            3 => format!("archive{i}.tar.zst"),
            4 => format!("src{i}.rs"),
            5 => format!("clip {i}.mp4"),
            _ => format!("file{i}.txt"),
        };
        std::fs::write(dir.join(name), i.to_string())?;
    }
    for i in 0..(n / 100).max(1) {
        std::fs::create_dir_all(dir.join(format!("folder {i:04}")))?;
    }
    Ok(())
}

fn deep(dir: &Path) -> std::io::Result<()> {
    // 100 × 10 directories × 100 files = 100,000 files, 1,100 directories
    for a in 0..100 {
        for b in 0..10 {
            let d = dir.join(format!("d{a:03}")).join(format!("s{b}"));
            std::fs::create_dir_all(&d)?;
            for f in 0..100 {
                std::fs::write(d.join(format!("f{f:03}.txt")), b"x")?;
            }
        }
    }
    Ok(())
}

fn photos(dir: &Path, n: usize) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for i in 0..n {
        // 256×256 PNG with a gradient so the thumbnail pipeline has real pixels to scale
        let mut img = image::RgbImage::new(256, 256);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = image::Rgb([(x + i as u32) as u8, y as u8, ((x ^ y) as u8).wrapping_mul(3)]);
        }
        img.save(dir.join(format!("IMG_{i:04}.png"))).map_err(std::io::Error::other)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- measurement

fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

fn rss_peak_mb() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    // macOS reports bytes, Linux kibibytes
    let bytes = if cfg!(target_os = "macos") { ru.ru_maxrss as f64 } else { ru.ru_maxrss as f64 * 1024.0 };
    (bytes / (1024.0 * 1024.0) * 10.0).round() / 10.0
}

fn wait_reset(rx: &mpsc::Receiver<Value>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(ev) if ev.str_field("event") == Some("Reset") => return true,
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return false,
        }
    }
    false
}

/// Every metric for one directory. Keys ending in `_ms` are compared as "lower is better".
pub fn measure(dir: &Path) -> BTreeMap<String, Value> {
    let mut m: BTreeMap<String, Value> = BTreeMap::new();
    let put = |m: &mut BTreeMap<String, Value>, k: &str, v: f64| {
        m.insert(k.into(), Value::Float(v));
    };
    let uri = Uri::from_path(dir);
    // fresh listing: phase 1, first chunk, first window
    listing::forget(&uri);
    let t0 = Instant::now();
    let (l, _) = listing::open(&uri).expect("open");
    let (tx, rx) = mpsc::channel();
    l.subscribe(Subscriber { client: 0, lid: 1, tx, first: 0, count: 60 });
    let (mut first_chunk, mut done) = (None, None);
    while done.is_none() {
        match rx.recv_timeout(Duration::from_secs(120)) {
            Ok(ev) if ev.str_field("event") == Some("Count") => {
                first_chunk.get_or_insert(t0.elapsed());
                if ev.get("done").and_then(Value::as_bool) == Some(true) {
                    done = Some(t0.elapsed());
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let (n, _) = l.count();
    put(&mut m, "entries", n as f64);
    put(&mut m, "phase1_first_chunk_ms", ms(first_chunk.unwrap_or_default()));
    put(&mut m, "phase1_done_ms", ms(done.unwrap_or_default()));
    let t = Instant::now();
    let w = l.window(0, 1, 0, 60);
    put(&mut m, "window_ms", ms(t.elapsed()));
    put(&mut m, "window_rows", w.get("rows").and_then(Value::as_arr).map(|a| a.len()).unwrap_or(0) as f64);
    // phase 2 for the first window: wait until every row has meta
    let t = Instant::now();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let w = l.window(0, 1, 0, 60);
        let all = w.get("rows").and_then(Value::as_arr).map(|a| a.iter().all(|r| r.get("meta").map(|v| *v != Value::Null).unwrap_or(false))).unwrap_or(true);
        if all || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    put(&mut m, "window_meta_ms", ms(t.elapsed()));
    let _ = rx.try_iter().count();
    // enrich everything, then sort by size and by mtime, then filter
    let t = Instant::now();
    let (etx, erx) = mpsc::channel();
    l.enrich(Some((etx, 1)));
    let _ = erx.recv_timeout(Duration::from_secs(600));
    put(&mut m, "enrich_ms", ms(t.elapsed()));
    for (role, key) in [(SortRole::Size, "sort_size_ms"), (SortRole::Mtime, "sort_mtime_ms"), (SortRole::Name, "sort_name_ms")] {
        let _ = rx.try_iter().count();
        let t = Instant::now();
        l.sort(role, false, None);
        wait_reset(&rx, Duration::from_secs(60));
        put(&mut m, key, ms(t.elapsed()));
    }
    let _ = rx.try_iter().count();
    let t = Instant::now();
    l.filter("photo");
    put(&mut m, "filter_ms", ms(t.elapsed()));
    l.filter("");
    // rescan keeping meta, then an in-place patch of one added and one removed name
    let t = Instant::now();
    l.rescan();
    put(&mut m, "rescan_ms", ms(t.elapsed()));
    let added = dir.join("zz-bench-added.txt");
    let _ = std::fs::write(&added, b"x");
    let t = Instant::now();
    l.patch(&[b"zz-bench-added.txt".to_vec()], &[], &[]);
    put(&mut m, "patch_ms", ms(t.elapsed()));
    let _ = std::fs::remove_file(&added);
    l.patch(&[], &[b"zz-bench-added.txt".to_vec()], &[]);
    l.unsubscribe(0, 1);
    // search index over this tree
    let t = Instant::now();
    let ix = crate::index::build(std::slice::from_ref(&dir.to_path_buf()), &[], &AtomicBool::new(false));
    put(&mut m, "index_build_ms", ms(t.elapsed()));
    put(&mut m, "index_entries", ix.len() as f64);
    let t = Instant::now();
    for _ in 0..100 {
        let _ = crate::index::query(&ix, "photo_0001", crate::index::Mode::Prefix);
    }
    put(&mut m, "index_query_us", (t.elapsed().as_secs_f64() * 1e6 / 100.0 * 10.0).round() / 10.0);
    // mirror scan + diff against an empty replica (local ↔ local)
    let replica = std::env::temp_dir().join(format!("kiki-bench-replica-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&replica);
    std::fs::create_dir_all(&replica).unwrap();
    if let Ok(mut spec) = crate::mirror::Spec::from_json(&Value::obj().s("master", uri.to_string()).s("replica", Uri::from_path(&replica).to_string()).s("direction", "upload").done()) {
        let t = Instant::now();
        if let Ok(plan) = crate::mirror::scan(&mut spec, &AtomicBool::new(false)) {
            put(&mut m, "mirror_scan_ms", ms(t.elapsed()));
            put(&mut m, "mirror_actions", plan.actions.len() as f64);
        }
    }
    let _ = std::fs::remove_dir_all(&replica);
    // thumbnails for the first 50 images, if any
    let images: Vec<PathBuf> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().map(|e| e == "png" || e == "jpg").unwrap_or(false)).take(50).collect()).unwrap_or_default();
    if !images.is_empty() {
        let t = Instant::now();
        let mut made = 0;
        for p in &images {
            let mt = std::fs::metadata(p).ok().and_then(|md| md.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
            if crate::thumbs::generate(&Uri::from_path(p), crate::kinds::Kind::Image, crate::thumbs::Size::Normal, mt).is_some() {
                made += 1;
            }
        }
        put(&mut m, "thumbs_ms", ms(t.elapsed()));
        put(&mut m, "thumbs_made", made as f64);
    }
    // a 64 MiB copy through the job copier
    let src = std::env::temp_dir().join(format!("kiki-bench-src-{}", std::process::id()));
    let dst = std::env::temp_dir().join(format!("kiki-bench-dst-{}", std::process::id()));
    let _ = std::fs::remove_file(&dst);
    if std::fs::write(&src, vec![7u8; 64 * 1024 * 1024]).is_ok() {
        let cancel = AtomicBool::new(false);
        let mut bytes = 0u64;
        let mut p = crate::ops::Progress { cancel: &cancel, bytes: &mut |n| bytes += n };
        let t = Instant::now();
        if crate::ops::copy_file(&src, &dst, &mut p).is_ok() {
            let el = t.elapsed();
            put(&mut m, "copy64m_ms", ms(el));
            put(&mut m, "copy_mb_s", (64.0 / el.as_secs_f64().max(1e-6) * 10.0).round() / 10.0);
        }
    }
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&dst);
    put(&mut m, "rss_peak_mb", rss_peak_mb());
    listing::forget(&uri);
    m
}

pub fn run(dir: &Path) -> Value {
    let mut results = BTreeMap::new();
    let profiles: Vec<(String, PathBuf)> = {
        let found: Vec<(String, PathBuf)> = PROFILES.iter().map(|p| (p.to_string(), dir.join(p))).filter(|(_, p)| p.is_dir()).collect();
        if found.is_empty() {
            vec![(dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "dir".into()), dir.to_path_buf())]
        } else {
            found
        }
    };
    for (name, path) in profiles {
        eprintln!("bench {name} …");
        results.insert(name, Value::Obj(measure(&path)));
    }
    Value::obj().s("arch", std::env::consts::ARCH).s("os", std::env::consts::OS).u("at", crate::ops::unix_now()).s("version", env!("CARGO_PKG_VERSION")).v("results", Value::Obj(results)).done()
}

pub fn print_table(v: &Value) {
    if let Some(Value::Obj(results)) = v.get("results") {
        for (name, metrics) in results {
            println!("== {name}");
            if let Value::Obj(m) = metrics {
                for (k, val) in m {
                    println!("  {k:<24} {}", crate::json::to_string(val));
                }
            }
        }
    }
}

/// Compares `_ms` and `_us` metrics; returns the regressions (profile, metric, baseline, now).
pub fn compare(baseline: &Value, now: &Value, tolerance_pct: f64) -> Vec<(String, String, f64, f64)> {
    let mut out = Vec::new();
    let (Some(Value::Obj(b)), Some(Value::Obj(n))) = (baseline.get("results"), now.get("results")) else { return out };
    for (profile, bm) in b {
        let (Value::Obj(bm), Some(Value::Obj(nm))) = (bm, n.get(profile)) else { continue };
        for (k, bv) in bm {
            if !(k.ends_with("_ms") || k.ends_with("_us")) {
                continue;
            }
            let (Some(bf), Some(nf)) = (as_f64(bv), nm.get(k).and_then(as_f64)) else { continue };
            // ignore sub-millisecond noise
            if nf > bf * (1.0 + tolerance_pct / 100.0) && nf - bf > 1.0 {
                out.push((profile.clone(), k.clone(), bf, nf));
            }
        }
    }
    out
}

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Uint(u) => Some(*u as f64),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_flags_only_slower_timings() {
        let base = Value::obj().v("results", Value::obj().v("flat", Value::obj().v("phase1_done_ms", Value::Float(100.0)).v("entries", Value::Float(10.0)).v("index_query_us", Value::Float(20.0)).done()).done()).done();
        let now = Value::obj().v("results", Value::obj().v("flat", Value::obj().v("phase1_done_ms", Value::Float(140.0)).v("entries", Value::Float(9.0)).v("index_query_us", Value::Float(21.0)).done()).done()).done();
        let r = compare(&base, &now, 25.0);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].1, "phase1_done_ms");
        assert!(compare(&base, &now, 50.0).is_empty());
    }

    #[test]
    fn small_tree_measures_every_metric() {
        let _g = crate::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let d = std::env::temp_dir().join(format!("kiki-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        gen("photos", &d.join("photos")).unwrap();
        flat(&d.join("flat"), 300).unwrap();
        std::env::set_var("KIKI_CACHE_DIR", d.join("cache"));
        std::env::set_var("XDG_CACHE_HOME", d.join("cache"));
        let m = measure(&d.join("flat"));
        for k in [
            "entries",
            "phase1_done_ms",
            "window_ms",
            "window_meta_ms",
            "enrich_ms",
            "sort_size_ms",
            "filter_ms",
            "rescan_ms",
            "patch_ms",
            "index_build_ms",
            "index_query_us",
            "mirror_scan_ms",
            "copy64m_ms",
            "rss_peak_mb",
        ] {
            assert!(m.contains_key(k), "{k} missing");
        }
        assert_eq!(as_f64(&m["entries"]), Some(303.0));
        let p = measure(&d.join("photos"));
        assert!(p.contains_key("thumbs_ms"), "thumbnails measured");
        std::env::remove_var("KIKI_CACHE_DIR");
        std::env::remove_var("XDG_CACHE_HOME");
        let _ = std::fs::remove_dir_all(&d);
    }
}
