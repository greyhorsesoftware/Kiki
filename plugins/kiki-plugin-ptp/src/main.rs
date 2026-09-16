//! PTP location plugin (plan 17): cameras (and phones in camera mode) through `libgphoto2`.
//! Listing walks the camera's folder tree one folder per `Scan`; `Thumb` uses the protocol's
//! own preview so the icon view never pulls full images.
//!
//! The `extern "C"` block mirrors `gphoto2-camera.h`, `gphoto2-file.h` and `gphoto2-list.h`
//! (libgphoto2 2.5). First build on Omarchy: check `CameraFileInfo` against the header.

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use std::ffi::{c_char, c_int, c_ulong, c_void, CStr, CString};
use std::io::{Read, Write};
use std::sync::Mutex;

// ---------------------------------------------------------------- libgphoto2 FFI

type Camera = c_void;
type GPContext = c_void;
type CameraList = c_void;
type CameraFile = c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CameraFileInfoPreview {
    fields: c_int,
    status: c_int,
    size: u64,
    r#type: [c_char; 64],
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CameraFileInfoFile {
    fields: c_int,
    status: c_int,
    size: u64,
    r#type: [c_char; 64],
    width: u32,
    height: u32,
    permissions: c_int,
    mtime: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CameraFileInfoAudio {
    fields: c_int,
    status: c_int,
    size: u64,
    r#type: [c_char; 64],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CameraFileInfo {
    preview: CameraFileInfoPreview,
    file: CameraFileInfoFile,
    audio: CameraFileInfoAudio,
}

const GP_OK: c_int = 0;
const GP_FILE_TYPE_PREVIEW: c_int = 0;
const GP_FILE_TYPE_NORMAL: c_int = 1;
const GP_FILE_INFO_SIZE: c_int = 1 << 2;
const GP_FILE_INFO_MTIME: c_int = 1 << 6;
const GP_FILE_PERM_DELETE: c_int = 1 << 1;
const GP_ERROR_CAMERA_BUSY: c_int = -110;
const GP_ERROR_MODEL_NOT_FOUND: c_int = -105;
const GP_ERROR_IO_USB_CLAIM: c_int = -53;

#[link(name = "gphoto2")]
#[link(name = "gphoto2_port")]
extern "C" {
    fn gp_context_new() -> *mut GPContext;
    fn gp_context_unref(ctx: *mut GPContext);
    fn gp_camera_new(camera: *mut *mut Camera) -> c_int;
    fn gp_camera_init(camera: *mut Camera, ctx: *mut GPContext) -> c_int;
    fn gp_camera_exit(camera: *mut Camera, ctx: *mut GPContext) -> c_int;
    fn gp_camera_unref(camera: *mut Camera) -> c_int;
    fn gp_camera_folder_list_files(camera: *mut Camera, folder: *const c_char, list: *mut CameraList, ctx: *mut GPContext) -> c_int;
    fn gp_camera_folder_list_folders(camera: *mut Camera, folder: *const c_char, list: *mut CameraList, ctx: *mut GPContext) -> c_int;
    fn gp_camera_folder_make_dir(camera: *mut Camera, folder: *const c_char, name: *const c_char, ctx: *mut GPContext) -> c_int;
    fn gp_camera_folder_remove_dir(camera: *mut Camera, folder: *const c_char, name: *const c_char, ctx: *mut GPContext) -> c_int;
    fn gp_camera_folder_put_file(camera: *mut Camera, folder: *const c_char, filename: *const c_char, ty: c_int, file: *mut CameraFile, ctx: *mut GPContext) -> c_int;
    fn gp_camera_file_get_info(camera: *mut Camera, folder: *const c_char, file: *const c_char, info: *mut CameraFileInfo, ctx: *mut GPContext) -> c_int;
    fn gp_camera_file_get(camera: *mut Camera, folder: *const c_char, file: *const c_char, ty: c_int, camera_file: *mut CameraFile, ctx: *mut GPContext) -> c_int;
    fn gp_camera_file_delete(camera: *mut Camera, folder: *const c_char, file: *const c_char, ctx: *mut GPContext) -> c_int;
    fn gp_list_new(list: *mut *mut CameraList) -> c_int;
    fn gp_list_unref(list: *mut CameraList) -> c_int;
    fn gp_list_count(list: *mut CameraList) -> c_int;
    fn gp_list_get_name(list: *mut CameraList, index: c_int, name: *mut *const c_char) -> c_int;
    fn gp_file_new(file: *mut *mut CameraFile) -> c_int;
    fn gp_file_unref(file: *mut CameraFile) -> c_int;
    fn gp_file_get_data_and_size(file: *mut CameraFile, data: *mut *const c_char, size: *mut c_ulong) -> c_int;
    fn gp_file_set_data_and_size(file: *mut CameraFile, data: *mut c_char, size: c_ulong) -> c_int;
    fn gp_result_as_string(result: c_int) -> *const c_char;
    fn malloc(n: usize) -> *mut c_void;
}

fn gp_err(code: c_int) -> PluginError {
    let msg = unsafe { CStr::from_ptr(gp_result_as_string(code)) }.to_string_lossy().into_owned();
    match code {
        GP_ERROR_CAMERA_BUSY | GP_ERROR_IO_USB_CLAIM => PluginError::new("Busy", format!("the camera is claimed by another program: {msg}")),
        GP_ERROR_MODEL_NOT_FOUND => PluginError::network(format!("no camera found: {msg}")),
        _ => PluginError::network(msg),
    }
}

fn c(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| PluginError::invalid("path", "bad path"))
}

// ---------------------------------------------------------------- session

struct Open {
    cam: *mut Camera,
    ctx: *mut GPContext,
}
unsafe impl Send for Open {}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            gp_camera_exit(self.cam, self.ctx);
            gp_camera_unref(self.cam);
            gp_context_unref(self.ctx);
        }
    }
}

struct Ptp {
    session: Mutex<Option<Open>>,
}

fn norm(path: &str) -> String {
    let p = path.trim_end_matches('/');
    if p.is_empty() {
        "/".into()
    } else if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    }
}

fn split(path: &str) -> (String, String) {
    let p = norm(path);
    match p.rfind('/') {
        Some(0) => ("/".into(), p[1..].to_string()),
        Some(i) => (p[..i].to_string(), p[i + 1..].to_string()),
        None => ("/".into(), p),
    }
}

fn names(list: *mut CameraList) -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        for i in 0..gp_list_count(list) {
            let mut p: *const c_char = std::ptr::null();
            if gp_list_get_name(list, i, &mut p) == GP_OK && !p.is_null() {
                out.push(CStr::from_ptr(p).to_string_lossy().into_owned());
            }
        }
    }
    out
}

impl Ptp {
    fn with<T>(&self, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        let mut g = self.session.lock().unwrap();
        // A camera that went to sleep drops the connection: the session is reopened on the
        // next call, and a failed call closes it so the retry reconnects.
        if g.is_none() {
            *g = Some(open_camera()?);
        }
        let s = g.as_mut().unwrap();
        match f(s) {
            Err(e) if e.code == "Network" => {
                *g = None;
                Err(e)
            }
            r => r,
        }
    }

    fn info(s: &Open, folder: &str, file: &str) -> Result<Meta> {
        let (cf, cn) = (c(folder)?, c(file)?);
        let mut info: CameraFileInfo = unsafe { std::mem::zeroed() };
        let r = unsafe { gp_camera_file_get_info(s.cam, cf.as_ptr(), cn.as_ptr(), &mut info, s.ctx) };
        if r != GP_OK {
            return Err(gp_err(r));
        }
        let size = if info.file.fields & GP_FILE_INFO_SIZE != 0 { info.file.size } else { 0 };
        let mtime = if info.file.fields & GP_FILE_INFO_MTIME != 0 { (info.file.mtime.max(0) as u64) * 1000 } else { 0 };
        Ok(Meta { size, mtime_ms: mtime, mode: None, owner: None, group: None })
    }

    fn fetch(s: &Open, folder: &str, file: &str, ty: c_int, out: &mut dyn Write) -> Result<()> {
        let (cf, cn) = (c(folder)?, c(file)?);
        unsafe {
            let mut f: *mut CameraFile = std::ptr::null_mut();
            gp_file_new(&mut f);
            let r = gp_camera_file_get(s.cam, cf.as_ptr(), cn.as_ptr(), ty, f, s.ctx);
            if r != GP_OK {
                gp_file_unref(f);
                return Err(gp_err(r));
            }
            let mut data: *const c_char = std::ptr::null();
            let mut size: c_ulong = 0;
            gp_file_get_data_and_size(f, &mut data, &mut size);
            let bytes = std::slice::from_raw_parts(data as *const u8, size as usize);
            let w = out.write_all(bytes);
            gp_file_unref(f);
            w.map_err(PluginError::io)
        }
    }
}

fn open_camera() -> Result<Open> {
    unsafe {
        let ctx = gp_context_new();
        let mut cam: *mut Camera = std::ptr::null_mut();
        let r = gp_camera_new(&mut cam);
        if r != GP_OK {
            gp_context_unref(ctx);
            return Err(gp_err(r));
        }
        let r = gp_camera_init(cam, ctx);
        if r != GP_OK {
            gp_camera_unref(cam);
            gp_context_unref(ctx);
            return Err(gp_err(r));
        }
        Ok(Open { cam, ctx })
    }
}

impl Handler for Ptp {
    fn describe(&self) -> Describe {
        Describe {
            scheme: "ptp",
            display_name: "Camera (PTP)",
            version: "0.1",
            form: vec![sdk::field("serial", "Serial", "text", false, None)],
            defaults: Value::obj().done(),
            secret_fields: vec![],
            detector_upload: "sizeOnly",
            detector_download: "sizeOnly",
            features: Features { set_mtime: false, mode: false, real_dirs: true, meta_in_scan: true, pipelining: false, partial_read: false },
        }
    }

    fn validate(&self, _config: &Value) -> Result<()> {
        Ok(())
    }

    fn connect(&self, _location: &str, _role: &str, _config: &Value, _secrets: &Value) -> Result<Value> {
        let mut g = self.session.lock().unwrap();
        if g.is_none() {
            *g = Some(open_camera()?);
        }
        Ok(Value::obj().v("fingerprint", Value::Null).v("banner", Value::Null).done())
    }

    fn disconnect(&self, _location: &str, role: &str) {
        if role == "browse" {
            *self.session.lock().unwrap() = None;
        }
    }

    fn capabilities(&self, _location: &str) -> Result<Value> {
        Ok(Value::obj().b("trash", false).b("setMtime", false).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").s("fastScan", "none").b("partialRead", false).done())
    }

    fn scan(&self, _location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        if recursive {
            return Err(PluginError::unsupported());
        }
        self.with(|s| {
            let folder = norm(path);
            let cf = c(&folder)?;
            let mut entries = Vec::new();
            unsafe {
                let mut list: *mut CameraList = std::ptr::null_mut();
                gp_list_new(&mut list);
                let r = gp_camera_folder_list_folders(s.cam, cf.as_ptr(), list, s.ctx);
                if r != GP_OK {
                    gp_list_unref(list);
                    return Err(gp_err(r));
                }
                for n in names(list) {
                    entries.push(Entry { name: n, kind: Kind::Dir, meta: Some(Meta { size: 0, mtime_ms: 0, mode: None, owner: None, group: None }), rel: String::new() });
                }
                gp_list_unref(list);
                let mut list: *mut CameraList = std::ptr::null_mut();
                gp_list_new(&mut list);
                let r = gp_camera_folder_list_files(s.cam, cf.as_ptr(), list, s.ctx);
                if r != GP_OK {
                    gp_list_unref(list);
                    return Err(gp_err(r));
                }
                let files = names(list);
                gp_list_unref(list);
                for n in files {
                    if sdk::cancelled() {
                        return Err(sdk::cancel_error());
                    }
                    let meta = Ptp::info(s, &folder, &n).ok();
                    entries.push(Entry { name: n, kind: Kind::File, meta, rel: String::new() });
                }
            }
            let n = entries.len() as u64;
            let mut it = entries.into_iter().peekable();
            while it.peek().is_some() {
                sink(it.by_ref().take(256).collect());
            }
            Ok(n)
        })
    }

    fn stat(&self, _location: &str, path: &str) -> Result<Meta> {
        let (folder, name) = split(path);
        if name.is_empty() {
            return Ok(Meta { size: 0, mtime_ms: 0, mode: None, owner: None, group: None });
        }
        self.with(|s| Ptp::info(s, &folder, &name))
    }

    fn read(&self, _location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        if offset != 0 {
            return Err(PluginError::unsupported());
        }
        let (folder, name) = split(path);
        self.with(|s| Ptp::fetch(s, &folder, &name, GP_FILE_TYPE_NORMAL, out))
    }

    fn thumb(&self, _location: &str, path: &str, out: &mut Outgoing) -> Result<()> {
        let (folder, name) = split(path);
        self.with(|s| Ptp::fetch(s, &folder, &name, GP_FILE_TYPE_PREVIEW, out))
    }

    fn write(&self, _location: &str, path: &str, mut args: WriteArgs) -> Result<u64> {
        let (folder, name) = split(path);
        let mut bytes = Vec::new();
        args.data.read_to_end(&mut bytes).map_err(PluginError::io)?;
        self.with(|s| {
            let (cf, cn) = (c(&folder)?, c(&name)?);
            unsafe {
                let mut f: *mut CameraFile = std::ptr::null_mut();
                gp_file_new(&mut f);
                // gp_file_set_data_and_size takes ownership of a malloc'd buffer.
                let buf = malloc(bytes.len().max(1)) as *mut c_char;
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), buf as *mut u8, bytes.len());
                gp_file_set_data_and_size(f, buf, bytes.len() as c_ulong);
                let r = gp_camera_folder_put_file(s.cam, cf.as_ptr(), cn.as_ptr(), GP_FILE_TYPE_NORMAL, f, s.ctx);
                gp_file_unref(f);
                if r != GP_OK {
                    return Err(if r == -6 { PluginError::unsupported() } else { gp_err(r) });
                }
            }
            Ok(bytes.len() as u64)
        })
    }

    fn mkdir(&self, _location: &str, path: &str) -> Result<()> {
        let (folder, name) = split(path);
        self.with(|s| {
            let (cf, cn) = (c(&folder)?, c(&name)?);
            let r = unsafe { gp_camera_folder_make_dir(s.cam, cf.as_ptr(), cn.as_ptr(), s.ctx) };
            if r != GP_OK {
                return Err(if r == -6 { PluginError::unsupported() } else { gp_err(r) });
            }
            Ok(())
        })
    }

    fn rename(&self, _location: &str, _from: &str, _to: &str) -> Result<()> {
        Err(PluginError::unsupported())
    }

    fn delete(&self, _location: &str, path: &str) -> Result<()> {
        let (folder, name) = split(path);
        self.with(|s| {
            let (cf, cn) = (c(&folder)?, c(&name)?);
            // folders: try remove_dir, files: check the delete permission bit first
            let mut info: CameraFileInfo = unsafe { std::mem::zeroed() };
            let is_file = unsafe { gp_camera_file_get_info(s.cam, cf.as_ptr(), cn.as_ptr(), &mut info, s.ctx) } == GP_OK;
            let r = if is_file {
                if info.file.permissions & GP_FILE_PERM_DELETE == 0 && info.file.fields != 0 {
                    return Err(PluginError::new("Denied", "the camera marks this file read-only"));
                }
                unsafe { gp_camera_file_delete(s.cam, cf.as_ptr(), cn.as_ptr(), s.ctx) }
            } else {
                unsafe { gp_camera_folder_remove_dir(s.cam, cf.as_ptr(), cn.as_ptr(), s.ctx) }
            };
            if r != GP_OK {
                return Err(if r == -6 { PluginError::unsupported() } else { gp_err(r) });
            }
            Ok(())
        })
    }
}

fn main() {
    let h = Ptp { session: Mutex::new(None) };
    if let Err(e) = sdk::run(&h) {
        eprintln!("kiki-plugin-ptp: {e}");
    }
}
