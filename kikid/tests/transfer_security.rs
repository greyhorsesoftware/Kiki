//! A hostile listing, through a real plugin process: what the daemon does with a name a server
//! chose. The unit tests in `mirror::safety` prove the checks; this proves they are *in the
//! path* a plugin's reply actually travels (docs/1.0.0/01-from-the-fork.md item 3).
mod common;
use kikid::{locations, mirror, vfs::uri::Uri};
use std::sync::atomic::AtomicBool;

/// Both scan paths, because the daemon has two: one recursive `Scan`, and the per-directory walk
/// it falls back to when a plugin cannot do the whole tree in one request. A plugin that answers
/// only the second would otherwise be an unchecked way in.
#[test]
fn a_listing_that_names_a_path_outside_the_root_is_refused() {
    let dir = common::setup("transfer-security");
    std::env::set_var("KIKI_STUB_UNSAFE_SCAN", "1");
    common::save_location("lab");
    let (session, _) = locations::resolve(&Uri::parse("stub://lab/").unwrap()).unwrap();
    let cancel = AtomicBool::new(false);
    for root in ["/unsafe-recursive", "/unsafe-fallback"] {
        let side = mirror::Side::Remote(session.clone(), root.into());
        let e = mirror::scan_side(&side, &[], &mut 0, &cancel).err().unwrap_or_else(|| panic!("{root}: a scan that names `../outside` must fail, not be walked"));
        assert!(e.message().contains("not inside the transfer root"), "{root}: said {:?}", e.message());
    }
    // And the scan of an ordinary folder still works, so the check is not refusing everything.
    let ok = mirror::Side::Remote(session, "/docs".into());
    assert!(mirror::scan_side(&ok, &[], &mut 0, &cancel).is_ok(), "an ordinary listing still scans");
    drop(dir);
}
