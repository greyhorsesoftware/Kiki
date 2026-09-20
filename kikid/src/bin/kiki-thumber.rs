//! `kiki-thumber`: makes thumbnails, and is the only part of kiki that decodes a file's contents.
//!
//! Every other thing the daemon reads it wrote itself or asked the kernel for. A thumbnail means
//! handing a picture, a video or a PDF — a file from anywhere, of any shape — to a decoder, and a
//! malformed one can take that decoder down or wedge it. In the daemon that is the whole file
//! manager: every window's listing, every running job. Here it is one process the daemon starts
//! again, and the file that did it is remembered so it is not offered twice.
//!
//! It speaks the framing of `API-PLUGIN.md` — the one way kiki runs a child — although it is not a
//! location plugin and has no `Describe`, `Connect`, or location of its own. The daemon runs it
//! through `Plugin::spawn_path`, which is there for exactly this: a binary that speaks the framing
//! and nothing more.
//!
//!     -> {"id":7,"type":"Thumb","uri":"file:///p/a.jpg","kind":"image","size":128,"mtime":170…}
//!     <- {"id":7,"ok":{"path":"/home/u/.cache/thumbnails/normal/<md5>.png"}}
//!     <- {"id":7,"err":{"code":"Unsupported","message":"no thumbnail"}}
//!
//! The pixels never cross the pipe: this process writes the cache file (and the freedesktop
//! failure marker, when there is nothing to be made) and answers with its path.

use kikid::json::Value;
use kikid::kinds::Kind;
use kikid::proto::{self, Frame, Framing, Reader};
use kikid::thumbs::{self, Size};
use kikid::vfs::uri::Uri;
use std::io::Write;
use std::sync::{mpsc, Arc, Mutex};

fn main() {
    // The whole process is background work: it must never take the machine from the window it is
    // drawing for. Set on the process, so each worker thread inherits it.
    #[cfg(target_os = "linux")]
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let (tx, rx) = mpsc::channel::<Value>();
    let rx = Arc::new(Mutex::new(rx));
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4);
    for i in 0..n {
        let rx = Arc::clone(&rx);
        let out = Arc::clone(&out);
        std::thread::Builder::new()
            .name(format!("thumb-{i}"))
            .spawn(move || loop {
                let job = { rx.lock().unwrap().recv() };
                let Ok(job) = job else { return };
                let id = job.u64_field("id").unwrap_or(0);
                // "There is no thumbnail for this file" is an **answer**, so it is an `ok` with a
                // null path — never an `err`. An `err` and a death down the pipe reach the daemon
                // as the same `Err`, and it must be able to tell "this file has no picture in it"
                // from "this process just died on it": one is forgotten, the other is a strike.
                let reply = proto::ok(id, Value::obj().v("path", make(&job).map(Value::Str).unwrap_or(Value::Null)).done());
                let mut o = out.lock().unwrap();
                let _ = proto::write_json(&mut *o, Framing::Binary, &reply);
                let _ = o.flush();
            })
            .expect("spawn thumb worker");
    }
    let mut reader = Reader::new(std::io::stdin());
    loop {
        match reader.next() {
            Ok(Some(Frame::Json(v))) => {
                if v.str_field("type") == Some("Shutdown") {
                    let mut o = out.lock().unwrap();
                    let _ = proto::write_json(&mut *o, Framing::Binary, &proto::ok(v.u64_field("id").unwrap_or(0), Value::obj().done()));
                    let _ = o.flush();
                    break;
                }
                if tx.send(v).is_err() {
                    break;
                }
            }
            // Nothing is ever sent here as bytes, and the daemon closing its end is how this ends.
            Ok(Some(Frame::Binary(_))) => continue,
            Ok(None) | Err(_) => break,
        }
    }
    drop(tx);
}

fn make(job: &Value) -> Option<String> {
    let uri = Uri::parse(job.str_field("uri")?).ok()?;
    let kind = match job.str_field("kind")? {
        "image" => Kind::Image,
        "video" => Kind::Video,
        "pdf" => Kind::Pdf,
        _ => return None,
    };
    let size = if job.u64_field("size") == Some(256) { Size::Large } else { Size::Normal };
    let mtime = job.u64_field("mtime").unwrap_or(0);
    thumbs::generate(&uri, kind, size, mtime).map(|p| p.to_string_lossy().into_owned())
}
