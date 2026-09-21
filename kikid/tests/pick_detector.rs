//! `mirror::pick_detector`, per plugin kind and per direction — what decides whether two files
//! are the same file, for every backend this build ships.
//!
//! It is asked of the plugin on the REMOTE side of the mirror, which is the replica when
//! uploading and the master when downloading, and each plugin answers per direction: FTP can be
//! trusted about a file's time when reading a listing but cannot set one it has just written, so
//! FTPS compares size and time on the way down and size alone on the way up. Getting that pairing
//! the wrong way round makes a mirror either re-copy everything on every run or miss a changed
//! file, so this drives the real plugin binaries: what they declare is half the answer, and it is
//! pinned here too.

mod common;

use kikid::mirror::{pick_detector, Detector, Direction, Spec};
use kikid::plugin;
use kikid::vfs::uri::Uri;
use std::path::{Path, PathBuf};

/// One line of the table below: the kind, what its plugin declares (upload, download), and what
/// the daemon then uses (upload, download).
type Row = (&'static str, (&'static str, &'static str), (Detector, Detector));

/// Where cargo put this build's binaries: this test runs from `<profile>/deps`.
fn built_binaries() -> PathBuf {
    let exe = std::env::current_exe().expect("current exe");
    exe.parent().and_then(Path::parent).expect("…/<profile>/deps/<test>").to_path_buf()
}

fn spec(master: &str, replica: &str, direction: Direction) -> Spec {
    Spec {
        master: Uri::parse(master).unwrap(),
        replica: Uri::parse(replica).unwrap(),
        direction,
        delete_extras: false,
        blast_radius: 0.5,
        confirmed_large_delete: false,
        clock_offset_ms: 0,
        clock_offset_auto: false,
        detector: Detector::Auto,
        modified_within_ms: None,
        apply_filters: false,
    }
}

/// What the daemon would use for this kind, mirroring a local folder each way.
fn for_kind(kind: &str) -> (Detector, Detector) {
    let up = spec("file:///m", &format!("{kind}://server/r"), Direction::Upload);
    let down = spec(&format!("{kind}://server/m"), "file:///r", Direction::Download);
    (pick_detector(&up), pick_detector(&down))
}

/// What the plugin itself says, straight out of its `Describe`.
fn declared(kind: &str) -> (String, String) {
    let d = plugin::describe(kind).unwrap_or_else(|| panic!("{kind} answered no Describe"));
    let det = d.get("detector").unwrap_or_else(|| panic!("{kind} declares no detector"));
    (det.str_field("upload").unwrap_or("").to_string(), det.str_field("download").unwrap_or("").to_string())
}

#[test]
fn every_plugin_kind_and_both_directions() {
    let dir = common::setup("detector");
    // The stub answers whatever it is told to, which is how the two verdicts no shipped plugin
    // asks for — a digest, and a word the daemon does not know — are driven through a real
    // `Describe` rather than a table written out here.
    std::env::set_var("KIKI_STUB_DETECTOR_UPLOAD", "digest");
    std::env::set_var("KIKI_STUB_DETECTOR_DOWNLOAD", "runes");

    // The location kinds this build ships, each under the name it is installed as: `smb` is the
    // gio binary run under that name (plan 25), and a kind is the suffix of that name.
    let built = built_binaries();
    let mut kinds: Vec<&str> = vec!["stub"];
    for (kind, bin) in [("sftp", "kiki-plugin-sftp"), ("ftps", "kiki-plugin-ftps"), ("smb", "kiki-plugin-gio")] {
        if !built.join(bin).is_file() {
            eprintln!("{bin} is not built here; {kind} skipped");
            continue;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(built.join(bin), dir.join("plugins").join(format!("kiki-plugin-{kind}"))).unwrap();
        kinds.push(kind);
    }
    assert!(kinds.len() > 1, "no location plugin was built: run `cargo test` over the workspace");
    let mut available = plugin::available();
    available.sort();
    let mut want: Vec<String> = kinds.iter().map(|k| k.to_string()).collect();
    want.sort();
    assert_eq!(available, want, "every kind put there is a kind the daemon can see");

    // (kind, what it declares, what the daemon then uses) — upload first, download second.
    #[rustfmt::skip]
    let table: Vec<Row> = vec![
        // SFTP carries a real mtime and can set one, so size and time both ways.
        ("sftp", ("sizeMtime", "sizeMtime"), (Detector::SizeMtime, Detector::SizeMtime)),
        // FTP cannot stamp a file it has just written, so an upload compares size alone; a
        // listing's time is good enough coming down.
        ("ftps", ("sizeOnly",  "sizeMtime"), (Detector::SizeOnly,  Detector::SizeMtime)),
        // gio. GVfs reports the server's times and keeps them.
        ("smb",  ("sizeMtime", "sizeMtime"), (Detector::SizeMtime, Detector::SizeMtime)),
        // The stub, told to ask for a digest one way and for nonsense the other: a word the
        // daemon does not know is size and time, which is the answer that is never wrong, only
        // slow.
        ("stub", ("digest",    "runes"),     (Detector::Digest,    Detector::SizeMtime)),
    ];
    for (kind, says, picks) in table {
        if !kinds.contains(&kind) {
            continue;
        }
        assert_eq!(declared(kind), (says.0.to_string(), says.1.to_string()), "{kind} declares what it always has");
        assert_eq!(for_kind(kind), picks, "{kind}: upload then download");
    }

    // ---------------------------------------------------------------- and what is not a plugin
    // Both ends on this machine: nothing to ask, and size and time is what local mirrors use.
    for d in [Direction::Upload, Direction::Download] {
        assert_eq!(pick_detector(&spec("file:///m", "file:///r", d)), Detector::SizeMtime);
    }
    // A scheme with no plugin here answers nothing, and that is not a reason to compare nothing.
    assert_eq!(for_kind("nosuch"), (Detector::SizeMtime, Detector::SizeMtime));
    // What the user chose beats every plugin's preference, in either direction.
    for kind in &kinds {
        for (direction, mut s) in [(Direction::Upload, spec("file:///m", &format!("{kind}://server/r"), Direction::Upload)), (Direction::Download, spec(&format!("{kind}://server/m"), "file:///r", Direction::Download))] {
            for chosen in [Detector::SizeOnly, Detector::SizeMtime, Detector::Digest] {
                s.detector = chosen;
                assert_eq!(pick_detector(&s), chosen, "{kind} {direction:?}: an explicit choice is kept");
            }
        }
    }

    std::env::remove_var("KIKI_STUB_DETECTOR_UPLOAD");
    std::env::remove_var("KIKI_STUB_DETECTOR_DOWNLOAD");
    std::fs::remove_dir_all(&dir).unwrap();
}
