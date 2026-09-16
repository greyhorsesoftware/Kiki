//! MTP location plugin (plan 17): Android phones and other MTP devices through `libmtp`.
//! The device is opened uncached (no full enumeration) and every folder is listed on demand
//! with the per-folder call, so a phone with thousands of photos connects in a moment. One
//! session per device; browse and job roles share it behind a mutex (MTP is single-client).
//!
//! The `extern "C"` block mirrors `libmtp.h` (1.1.x). First build on Omarchy: check the
//! struct layouts and the `LIBMTP_FILETYPE_*` values against the installed header.

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::io::{Read, Write};
use std::sync::Mutex;

// ---------------------------------------------------------------- libmtp FFI

#[repr(C)]
struct DeviceEntry {
    vendor: *mut c_char,
    vendor_id: u16,
    product: *mut c_char,
    product_id: u16,
    device_flags: u32,
}

#[repr(C)]
struct RawDevice {
    device_entry: DeviceEntry,
    bus_location: u32,
    devnum: u8,
}

#[repr(C)]
struct Storage {
    id: u32,
    storage_type: u16,
    filesystem_type: u16,
    access_capability: u16,
    max_capacity: u64,
    free_space_in_bytes: u64,
    free_space_in_objects: u64,
    storage_description: *mut c_char,
    volume_identifier: *mut c_char,
    next: *mut Storage,
    prev: *mut Storage,
}

/// Prefix of `LIBMTP_mtpdevice_t`; only `storage` is read, the rest stays opaque.
#[repr(C)]
struct MtpDevice {
    object_bitsize: u8,
    params: *mut c_void,
    usbinfo: *mut c_void,
    storage: *mut Storage,
}

#[repr(C)]
struct File {
    item_id: u32,
    parent_id: u32,
    storage_id: u32,
    filename: *mut c_char,
    filesize: u64,
    modificationdate: libc_time_t,
    filetype: c_int,
    next: *mut File,
}

#[allow(non_camel_case_types)]
type libc_time_t = i64;

const FILETYPE_FOLDER: c_int = 0;
const FILETYPE_UNKNOWN: c_int = 44;
const PARENT_ROOT: u32 = 0;
const HANDLER_OK: u16 = 0;
const HANDLER_ERROR: u16 = 1;

type DataGetFunc = extern "C" fn(params: *mut c_void, private: *mut c_void, wantlen: u32, data: *mut u8, gotlen: *mut u32) -> u16;
type DataPutFunc = extern "C" fn(params: *mut c_void, private: *mut c_void, sendlen: u32, data: *mut u8, putlen: *mut u32) -> u16;

#[link(name = "mtp")]
extern "C" {
    fn LIBMTP_Init();
    fn LIBMTP_Detect_Raw_Devices(devices: *mut *mut RawDevice, numdevs: *mut c_int) -> c_int;
    fn LIBMTP_Open_Raw_Device_Uncached(rawdevice: *mut RawDevice) -> *mut MtpDevice;
    fn LIBMTP_Release_Device(device: *mut MtpDevice);
    fn LIBMTP_Get_Serialnumber(device: *mut MtpDevice) -> *mut c_char;
    fn LIBMTP_Get_Friendlyname(device: *mut MtpDevice) -> *mut c_char;
    fn LIBMTP_Get_Storage(device: *mut MtpDevice, sortby: c_int) -> c_int;
    fn LIBMTP_Get_Files_And_Folders(device: *mut MtpDevice, storage: u32, parent: u32) -> *mut File;
    fn LIBMTP_destroy_file_t(file: *mut File);
    fn LIBMTP_new_file_t() -> *mut File;
    fn LIBMTP_Get_File_To_Handler(device: *mut MtpDevice, id: u32, put_func: DataPutFunc, private: *mut c_void, callback: *const c_void, data: *const c_void) -> c_int;
    fn LIBMTP_Send_File_From_Handler(device: *mut MtpDevice, get_func: DataGetFunc, private: *mut c_void, filedata: *mut File, callback: *const c_void, data: *const c_void) -> c_int;
    fn LIBMTP_Delete_Object(device: *mut MtpDevice, id: u32) -> c_int;
    fn LIBMTP_Create_Folder(device: *mut MtpDevice, name: *mut c_char, parent: u32, storage: u32) -> u32;
    fn LIBMTP_Set_Object_Filename(device: *mut MtpDevice, id: u32, newname: *const c_char) -> c_int;
    fn LIBMTP_Clear_Errorstack(device: *mut MtpDevice);
    fn free(p: *mut c_void);
}

fn cstr(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}

// ---------------------------------------------------------------- session

struct Open {
    dev: *mut MtpDevice,
    /// path -> (storage id, object id); "" is the virtual root, "/<storage>" a storage root
    ids: HashMap<String, (u32, u32)>,
    storages: Vec<(String, u32)>,
}
unsafe impl Send for Open {}

struct Mtp {
    session: Mutex<Option<Open>>,
    name: Mutex<String>,
}

fn norm(path: &str) -> String {
    let p = path.trim_matches('/');
    if p.is_empty() {
        String::new()
    } else {
        format!("/{p}")
    }
}

impl Mtp {
    fn with<T>(&self, f: impl FnOnce(&mut Open) -> Result<T>) -> Result<T> {
        let mut g = self.session.lock().unwrap();
        let s = g.as_mut().ok_or_else(|| PluginError::network("not connected"))?;
        f(s)
    }

    /// The object id for a path, walking from the storage root one folder at a time.
    fn resolve(s: &mut Open, path: &str) -> Result<(u32, u32)> {
        let path = norm(path);
        if let Some(v) = s.ids.get(&path) {
            return Ok(*v);
        }
        let mut cur = String::new();
        let mut ids = (0u32, PARENT_ROOT);
        for seg in path.split('/').filter(|x| !x.is_empty()) {
            let next = format!("{cur}/{seg}");
            if let Some(v) = s.ids.get(&next) {
                ids = *v;
                cur = next;
                continue;
            }
            if cur.is_empty() {
                let st = s.storages.iter().find(|(n, _)| n == seg).ok_or_else(PluginError::not_found)?;
                ids = (st.1, PARENT_ROOT);
            } else {
                let mut found = None;
                for (name, id, is_dir, _, _) in list_children(s.dev, ids.0, ids.1) {
                    let child = format!("{cur}/{name}");
                    s.ids.insert(child, (ids.0, id));
                    if name == seg {
                        found = Some((ids.0, id, is_dir));
                    }
                }
                let (st, id, _) = found.ok_or_else(PluginError::not_found)?;
                ids = (st, id);
            }
            s.ids.insert(next.clone(), ids);
            cur = next;
        }
        Ok(ids)
    }
}

/// (name, id, is_dir, size, mtime_ms) for one folder.
fn list_children(dev: *mut MtpDevice, storage: u32, parent: u32) -> Vec<(String, u32, bool, u64, u64)> {
    let mut out = Vec::new();
    unsafe {
        let head = LIBMTP_Get_Files_And_Folders(dev, storage, parent);
        let mut p = head;
        while !p.is_null() {
            let f = &*p;
            out.push((cstr(f.filename), f.item_id, f.filetype == FILETYPE_FOLDER, f.filesize, (f.modificationdate.max(0) as u64) * 1000));
            let next = f.next;
            LIBMTP_destroy_file_t(p);
            p = next;
        }
        LIBMTP_Clear_Errorstack(dev);
    }
    out
}

struct PutCtx<'a> {
    out: &'a mut Outgoing<'a>,
    failed: bool,
}

extern "C" fn put_func(_params: *mut c_void, private: *mut c_void, sendlen: u32, data: *mut u8, putlen: *mut u32) -> u16 {
    let ctx = unsafe { &mut *(private as *mut PutCtx) };
    let bytes = unsafe { std::slice::from_raw_parts(data, sendlen as usize) };
    if ctx.out.write_all(bytes).is_err() {
        ctx.failed = true;
        return HANDLER_ERROR;
    }
    unsafe { *putlen = sendlen };
    HANDLER_OK
}

struct GetCtx<'a> {
    data: &'a mut dyn Read,
    failed: bool,
}

extern "C" fn get_func(_params: *mut c_void, private: *mut c_void, wantlen: u32, data: *mut u8, gotlen: *mut u32) -> u16 {
    let ctx = unsafe { &mut *(private as *mut GetCtx) };
    let buf = unsafe { std::slice::from_raw_parts_mut(data, wantlen as usize) };
    let mut total = 0usize;
    while total < buf.len() {
        match ctx.data.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => {
                ctx.failed = true;
                return HANDLER_ERROR;
            }
        }
    }
    unsafe { *gotlen = total as u32 };
    HANDLER_OK
}

impl Handler for Mtp {
    fn describe(&self) -> Describe {
        Describe {
            scheme: "mtp",
            display_name: "Android (MTP)",
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

    fn connect(&self, location: &str, _role: &str, config: &Value, _secrets: &Value) -> Result<Value> {
        let mut g = self.session.lock().unwrap();
        if g.is_some() {
            return Ok(Value::obj().v("fingerprint", Value::Null).opt_s("banner", Some(self.name.lock().unwrap().as_str())).done());
        }
        let want_serial = config.str_field("serial").unwrap_or("").to_string();
        let want_bus = config.u64_field("bus").unwrap_or(0) as u32;
        let want_dev = config.u64_field("devnum").unwrap_or(0) as u8;
        unsafe {
            LIBMTP_Init();
            let mut raw: *mut RawDevice = std::ptr::null_mut();
            let mut n: c_int = 0;
            let r = LIBMTP_Detect_Raw_Devices(&mut raw, &mut n);
            if r != 0 || n == 0 || raw.is_null() {
                return Err(PluginError::network(format!("no MTP device found ({location})")));
            }
            let devices = std::slice::from_raw_parts_mut(raw, n as usize);
            let mut chosen: *mut MtpDevice = std::ptr::null_mut();
            let mut name = String::new();
            // Prefer the bus address the daemon saw in sysfs; fall back to the serial, then the only device.
            let order: Vec<usize> = {
                let mut v: Vec<usize> = (0..devices.len()).collect();
                v.sort_by_key(|&i| if devices[i].bus_location == want_bus && devices[i].devnum == want_dev { 0 } else { 1 });
                v
            };
            for i in order {
                let dev = LIBMTP_Open_Raw_Device_Uncached(&mut devices[i]);
                if dev.is_null() {
                    continue;
                }
                let serial_p = LIBMTP_Get_Serialnumber(dev);
                let serial = cstr(serial_p);
                free(serial_p as *mut c_void);
                if want_serial.is_empty() || serial == want_serial || devices.len() == 1 {
                    let fp = LIBMTP_Get_Friendlyname(dev);
                    name = cstr(fp);
                    free(fp as *mut c_void);
                    chosen = dev;
                    break;
                }
                LIBMTP_Release_Device(dev);
            }
            free(raw as *mut c_void);
            if chosen.is_null() {
                return Err(PluginError::new("Busy", "the device is claimed by another program or refused the connection"));
            }
            if LIBMTP_Get_Storage(chosen, 0) != 0 {
                LIBMTP_Release_Device(chosen);
                return Err(PluginError::network("cannot read the device's storage list; is the phone unlocked and in file-transfer mode?"));
            }
            let mut storages = Vec::new();
            let mut st = (*chosen).storage;
            while !st.is_null() {
                let s = &*st;
                let label = cstr(s.storage_description);
                let label = if label.is_empty() { cstr(s.volume_identifier) } else { label };
                let label = if label.is_empty() { format!("Storage {}", s.id) } else { label };
                storages.push((label, s.id));
                st = s.next;
            }
            *self.name.lock().unwrap() = name.clone();
            *g = Some(Open { dev: chosen, ids: HashMap::new(), storages });
            Ok(Value::obj().v("fingerprint", Value::Null).s("banner", name).done())
        }
    }

    fn disconnect(&self, _location: &str, role: &str) {
        // Both roles share the session; the browse role owns it.
        if role != "browse" {
            return;
        }
        if let Some(s) = self.session.lock().unwrap().take() {
            unsafe { LIBMTP_Release_Device(s.dev) };
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
            let path = norm(path);
            if path.is_empty() {
                let entries: Vec<Entry> =
                    s.storages.iter().map(|(n, _)| Entry { name: n.clone(), kind: Kind::Dir, meta: Some(Meta { size: 0, mtime_ms: 0, mode: None, owner: None, group: None }), rel: String::new() }).collect();
                let n = entries.len() as u64;
                sink(entries);
                return Ok(n);
            }
            let (storage, parent) = Mtp::resolve(s, &path)?;
            let mut entries = Vec::new();
            for (name, id, is_dir, size, mtime) in list_children(s.dev, storage, parent) {
                if sdk::cancelled() {
                    return Err(sdk::cancel_error());
                }
                s.ids.insert(format!("{path}/{name}"), (storage, id));
                entries.push(Entry { name, kind: if is_dir { Kind::Dir } else { Kind::File }, meta: Some(Meta { size, mtime_ms: mtime, mode: None, owner: None, group: None }), rel: String::new() });
            }
            let n = entries.len() as u64;
            let mut it = entries.into_iter().peekable();
            while it.peek().is_some() {
                sink(it.by_ref().take(512).collect());
            }
            Ok(n)
        })
    }

    fn stat(&self, _location: &str, path: &str) -> Result<Meta> {
        self.with(|s| {
            let path = norm(path);
            let (dir, name) = match path.rfind('/') {
                Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
                None => return Ok(Meta { size: 0, mtime_ms: 0, mode: None, owner: None, group: None }),
            };
            if dir.is_empty() {
                return Ok(Meta { size: 0, mtime_ms: 0, mode: None, owner: None, group: None });
            }
            let (storage, parent) = Mtp::resolve(s, &dir)?;
            list_children(s.dev, storage, parent)
                .into_iter()
                .find(|(n, ..)| *n == name)
                .map(|(_, _, _, size, mtime)| Meta { size, mtime_ms: mtime, mode: None, owner: None, group: None })
                .ok_or_else(PluginError::not_found)
        })
    }

    fn read(&self, _location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        if offset != 0 {
            return Err(PluginError::unsupported()); // partialRead: false
        }
        self.with(|s| {
            let (_, id) = Mtp::resolve(s, path)?;
            // SAFETY: the callback only lives for the duration of the call below.
            let out: &mut Outgoing<'_> = unsafe { std::mem::transmute(out) };
            let mut ctx = PutCtx { out, failed: false };
            let r = unsafe { LIBMTP_Get_File_To_Handler(s.dev, id, put_func, &mut ctx as *mut PutCtx as *mut c_void, std::ptr::null(), std::ptr::null()) };
            if ctx.failed {
                return Err(if sdk::cancelled() { sdk::cancel_error() } else { PluginError::io("write to pipe failed") });
            }
            if r != 0 {
                return Err(PluginError::network("device refused the transfer"));
            }
            Ok(())
        })
    }

    fn write(&self, _location: &str, path: &str, mut args: WriteArgs) -> Result<u64> {
        self.with(|s| {
            let path = norm(path);
            let (dir, name) = path.rfind('/').map(|i| (path[..i].to_string(), path[i + 1..].to_string())).ok_or_else(|| PluginError::invalid("path", "cannot write at the root"))?;
            if dir.is_empty() {
                return Err(PluginError::invalid("path", "pick a storage first"));
            }
            let (storage, parent) = Mtp::resolve(s, &dir)?;
            // MTP needs the size up front: spool to a temp file when the daemon did not send it.
            let size = match args.size {
                Some(n) => n,
                None => {
                    let mut v = Vec::new();
                    args.data.read_to_end(&mut v).map_err(PluginError::io)?;
                    let n = v.len() as u64;
                    let mut cur = std::io::Cursor::new(v);
                    return send(s, storage, parent, &name, n, &mut cur, args.mtime_ms);
                }
            };
            send(s, storage, parent, &name, size, &mut args.data, args.mtime_ms)
        })
    }

    fn mkdir(&self, _location: &str, path: &str) -> Result<()> {
        self.with(|s| {
            let path = norm(path);
            let (dir, name) = path.rfind('/').map(|i| (path[..i].to_string(), path[i + 1..].to_string())).ok_or_else(|| PluginError::invalid("path", "bad path"))?;
            if dir.is_empty() {
                return Err(PluginError::invalid("path", "pick a storage first"));
            }
            let (storage, parent) = Mtp::resolve(s, &dir)?;
            let c = CString::new(name).map_err(|_| PluginError::invalid("path", "bad name"))?;
            let id = unsafe { LIBMTP_Create_Folder(s.dev, c.as_ptr().cast_mut(), parent, storage) };
            if id == 0 {
                return Err(PluginError::network("device refused to create the folder"));
            }
            s.ids.insert(path, (storage, id));
            Ok(())
        })
    }

    fn rename(&self, _location: &str, from: &str, to: &str) -> Result<()> {
        self.with(|s| {
            let (from_dir, _) = split(&norm(from));
            let (to_dir, new_name) = split(&norm(to));
            if from_dir != to_dir {
                return Err(PluginError::unsupported()); // MTP has no move; the daemon copies and deletes
            }
            let (_, id) = Mtp::resolve(s, from)?;
            let c = CString::new(new_name).map_err(|_| PluginError::invalid("path", "bad name"))?;
            let r = unsafe { LIBMTP_Set_Object_Filename(s.dev, id, c.as_ptr()) };
            s.ids.remove(&norm(from));
            if r != 0 {
                return Err(PluginError::network("rename refused"));
            }
            Ok(())
        })
    }

    fn delete(&self, _location: &str, path: &str) -> Result<()> {
        self.with(|s| {
            let (storage, id) = Mtp::resolve(s, path)?;
            if !list_children(s.dev, storage, id).is_empty() {
                return Err(PluginError::new("NotEmpty", "directory not empty"));
            }
            let r = unsafe { LIBMTP_Delete_Object(s.dev, id) };
            s.ids.remove(&norm(path));
            if r != 0 {
                return Err(PluginError::network("delete refused"));
            }
            Ok(())
        })
    }
}

fn split(p: &str) -> (String, String) {
    match p.rfind('/') {
        Some(i) => (p[..i].to_string(), p[i + 1..].to_string()),
        None => (String::new(), p.to_string()),
    }
}

fn send(s: &mut Open, storage: u32, parent: u32, name: &str, size: u64, data: &mut dyn Read, mtime_ms: Option<u64>) -> Result<u64> {
    let c = CString::new(name).map_err(|_| PluginError::invalid("path", "bad name"))?;
    unsafe {
        let f = LIBMTP_new_file_t();
        (*f).filesize = size;
        (*f).filename = libc_strdup(c.as_ptr());
        (*f).parent_id = parent;
        (*f).storage_id = storage;
        (*f).filetype = FILETYPE_UNKNOWN;
        (*f).modificationdate = mtime_ms.map(|m| (m / 1000) as i64).unwrap_or(0);
        let mut ctx = GetCtx { data, failed: false };
        let r = LIBMTP_Send_File_From_Handler(s.dev, get_func, &mut ctx as *mut GetCtx as *mut c_void, f, std::ptr::null(), std::ptr::null());
        LIBMTP_destroy_file_t(f);
        if ctx.failed {
            return Err(PluginError::io("read from pipe failed"));
        }
        if r != 0 {
            return Err(PluginError::network("device refused the upload"));
        }
    }
    Ok(size)
}

extern "C" {
    #[link_name = "strdup"]
    fn libc_strdup(s: *const c_char) -> *mut c_char;
}

fn main() {
    let h = Mtp { session: Mutex::new(None), name: Mutex::new(String::new()) };
    if let Err(e) = sdk::run(&h) {
        eprintln!("kiki-plugin-mtp: {e}");
    }
}
