//! Deciding whether two sides of a pair differ, and by how much the clocks disagree.

use super::*;

fn size_mtime(m: &Entry, r: &Entry, offset_ms: i64) -> bool {
    if m.size != r.size {
        return true;
    }
    if m.mtime_ms == 0 || r.mtime_ms == 0 {
        return false;
    }
    let adjusted = m.mtime_ms as i64 - offset_ms;
    (adjusted - r.mtime_ms as i64).abs() > TOLERANCE_MS
}

pub(super) fn usable_md5(d: &Option<String>) -> Option<String> {
    d.as_ref().filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())).map(|s| s.to_ascii_lowercase())
}

pub fn is_changed(det: Detector, m: &Entry, r: &Entry, offset_ms: i64) -> bool {
    match det {
        Detector::SizeOnly => m.size != r.size,
        Detector::Digest => {
            if m.size != r.size {
                return true;
            }
            match (usable_md5(&m.digest), usable_md5(&r.digest)) {
                (Some(a), Some(b)) => a != b,
                _ => size_mtime(m, r, offset_ms), // single-sided digests need a local MD5; not available here, fall back
            }
        }
        Detector::SizeMtime | Detector::Auto => size_mtime(m, r, offset_ms),
    }
}

/// Median of (master − replica) mtime deltas over same-size file pairs; 0 below three samples.
pub fn auto_offset(master: &SideMap, replica: &SideMap) -> i64 {
    let mut deltas: Vec<i64> = master
        .iter()
        .filter_map(|(rel, m)| {
            let r = replica.get(rel)?;
            if m.is_dir || r.is_dir || m.size != r.size || m.mtime_ms == 0 || r.mtime_ms == 0 {
                return None;
            }
            Some(m.mtime_ms as i64 - r.mtime_ms as i64)
        })
        .collect();
    if deltas.len() < 3 {
        return 0;
    }
    deltas.sort_unstable();
    let median = deltas[deltas.len() / 2];
    // A clock that is off moves EVERY pair by the same amount; the deltas agree. A side whose
    // files were all re-stamped at one moment — a `git checkout`, a copy — against a replica
    // written over months gives deltas all over the place, and their median is not a clock:
    // it was 9 days once (2026-09-24), printed in the report and subtracted from every mtime.
    // Unless more than half the samples sit within the tolerance of the median, there is no
    // offset to speak of.
    let agree = deltas.iter().filter(|d| (*d - median).abs() <= TOLERANCE_MS).count();
    if agree * 2 <= deltas.len() {
        return 0;
    }
    median
}

pub fn pick_detector(spec: &Spec) -> Detector {
    if spec.detector != Detector::Auto {
        return spec.detector;
    }
    // The remote side's plugin answers; local-only mirrors use size+mtime.
    let remote = if spec.direction == Direction::Upload { &spec.replica } else { &spec.master };
    if remote.is_local() {
        return Detector::SizeMtime;
    }
    let key = if spec.direction == Direction::Upload { "upload" } else { "download" };
    match crate::plugin::describe(&remote.scheme).and_then(|d| d.get("detector").and_then(|x| x.str_field(key).map(str::to_string))).as_deref() {
        Some("sizeOnly") => Detector::SizeOnly,
        Some("digest") => Detector::Digest,
        _ => Detector::SizeMtime,
    }
}
