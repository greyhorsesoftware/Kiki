//! Benchmarks (plan 26): synthetic trees, one timed pass over the daemon's hot paths, JSON
//! results, and a comparison that fails on regressions.
//!
//! `kikid bench gen <profile> <dir>`   flat10k | flat200k | deep100k | photos | gallery1k | all
//! `kikid bench run <dir> [--json out]` measure every profile found under <dir> (or one directory)
//! `kikid bench compare <baseline.json> <results.json> [--tolerance 25]`

use crate::json::Value;
use crate::listing::{self, SortRole, Subscriber};
use crate::vfs::uri::Uri;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Mutex;
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
        // What the gallery is asked to survive: a thousand full-size photographs, none of them
        // thumbnailed yet. Sized and encoded like something off a camera, not a test pattern.
        "gallery1k" => jpegs(dir, 1_000, 1_600, 1_200),
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

/// A folder of full-size JPEGs, written on every core there is — a thousand of them takes long
/// enough that doing it one at a time would dominate the run that measures them.
fn jpegs(dir: &Path, n: usize, w: u32, h: u32) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let threads = std::thread::available_parallelism().map(|p| p.get()).unwrap_or(4).min(n.max(1));
    let err = Mutex::new(None::<std::io::Error>);
    std::thread::scope(|scope| {
        for t in 0..threads {
            let err = &err;
            scope.spawn(move || {
                for i in (t..n).step_by(threads) {
                    let path = dir.join(format!("DSC_{i:05}.jpg"));
                    // Skip what is already there: the fixture is expensive and worth reusing.
                    if std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
                        continue;
                    }
                    let mut img = image::RgbImage::new(w, h);
                    let seed = i as u32;
                    for (x, y, p) in img.enumerate_pixels_mut() {
                        // Broad shapes with a little grain on top: white noise would not compress
                        // and would give every file the wrong size and the wrong decode cost.
                        let fx = x as f32 / w as f32;
                        let fy = y as f32 / h as f32;
                        let phase = seed as f32 * 0.37;
                        let a = ((fx * 6.0 + phase).sin() * (fy * 4.0 - phase).cos() + 1.0) * 96.0;
                        let b = ((fx * 2.0 - fy * 3.0 + phase).sin() + 1.0) * 110.0;
                        let grain = (((x * 31 + y * 17 + seed * 7) % 17) as f32) - 8.0;
                        *p = image::Rgb([
                            (a + grain + 40.0).clamp(0.0, 255.0) as u8,
                            (b + grain * 0.5 + 20.0).clamp(0.0, 255.0) as u8,
                            (255.0 - a * 0.7 + grain).clamp(0.0, 255.0) as u8,
                        ]);
                    }
                    if let Err(e) = img.save(&path) {
                        *err.lock().unwrap() = Some(std::io::Error::other(e));
                        return;
                    }
                }
            });
        }
    });
    match err.into_inner().unwrap() {
        Some(e) => Err(e),
        None => Ok(()),
    }
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

/// Current resident set in MB (Linux: /proc/self/statm; macOS: proc_pidinfo).
pub fn rss_now_mb() -> f64 {
    #[cfg(target_os = "linux")]
    {
        let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
        let pages: f64 = statm.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as f64;
        (pages * page / (1024.0 * 1024.0) * 10.0).round() / 10.0
    }
    #[cfg(target_os = "macos")]
    {
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let got = unsafe { libc::proc_pidinfo(libc::getpid(), libc::PROC_PIDTASKINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) };
        if got == size {
            return (info.pti_resident_size as f64 / (1024.0 * 1024.0) * 10.0).round() / 10.0;
        }
        0.0
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        0.0
    }
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
    put(&mut m, "rss_start_mb", rss_now_mb());
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
    put(&mut m, "rss_listing_mb", rss_now_mb());
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
    let wrote = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(&src)?;
        let chunk = vec![7u8; 1024 * 1024];
        for _ in 0..64 {
            f.write_all(&chunk)?;
        }
        Ok(())
    })()
    .is_ok();
    if wrote {
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
    // release everything and see what the process gives back (Linux trims; macOS mostly keeps it)
    listing::forget(&uri);
    drop(l);
    drop(ix);
    put(&mut m, "rss_after_release_mb", rss_now_mb());
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
    // Each profile runs in its own process so peaks and release numbers are independent.
    let child_ok = std::env::var_os("KIKI_BENCH_CHILD").is_none() && profiles.len() > 1;
    for (name, path) in profiles {
        eprintln!("bench {name} …");
        if child_ok {
            if let Some(v) = run_child(&path) {
                results.insert(name, v);
                continue;
            }
        }
        results.insert(name, Value::Obj(measure(&path)));
    }
    Value::obj()
        .s("arch", std::env::consts::ARCH)
        .s("os", std::env::consts::OS)
        .u("at", crate::ops::unix_now())
        .s("version", env!("CARGO_PKG_VERSION"))
        .s("build", env!("KIKI_BUILD"))
        .v("machine", machine(dir))
        .v("results", Value::Obj(results))
        .done()
}

/// Which machine produced these numbers. A baseline without it is a row of figures nobody can
/// reproduce: a listing benchmark measures the filesystem underneath it as much as the code.
fn machine(dir: &Path) -> Value {
    let cpu = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|t| t.lines().find(|l| l.starts_with("model name")).and_then(|l| l.split_once(':').map(|(_, v)| v.trim().to_string())))
        .unwrap_or_default();
    let mem_kb = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| t.lines().find(|l| l.starts_with("MemTotal:")).and_then(|l| l.split_whitespace().nth(1).and_then(|n| n.parse::<u64>().ok())))
        .unwrap_or(0);
    let kernel = std::process::Command::new("uname").arg("-r").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    // The filesystem the trees were generated on, which is half of what a listing benchmark measures.
    let fs = std::process::Command::new("findmnt")
        .args(["-no", "FSTYPE", "-T"])
        .arg(dir)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    Value::obj()
        .s("cpu", cpu)
        .u("cores", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0) as u64)
        .u("memMb", mem_kb / 1024)
        .s("kernel", kernel)
        .s("fs", fs)
        .s("dir", dir.to_string_lossy())
        .done()
}

fn run_child(path: &Path) -> Option<Value> {
    let exe = std::env::current_exe().ok()?;
    let out = std::env::temp_dir().join(format!("kiki-bench-child-{}-{}.json", std::process::id(), path.file_name()?.to_string_lossy()));
    let st = std::process::Command::new(exe).env("KIKI_BENCH_CHILD", "1").args(["bench", "run"]).arg(path).arg("--json").arg(&out).stdout(std::process::Stdio::null()).status().ok()?;
    if !st.success() {
        return None;
    }
    let v = crate::json::parse(&std::fs::read(&out).ok()?).ok()?;
    let _ = std::fs::remove_file(&out);
    let single = v.get("results").and_then(|r| if let Value::Obj(m) = r { m.values().next().cloned() } else { None })?;
    Some(single)
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
            // ignore noise: a regression must clear the tolerance and an absolute floor
            // (3 ms for timings, 50 µs for the microsecond metrics)
            let floor = if k.ends_with("_us") { 50.0 } else { 3.0 };
            if nf > bf * (1.0 + tolerance_pct / 100.0) && nf - bf > floor {
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
