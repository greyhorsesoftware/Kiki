//! AFC location plugin (plan 17): iPhone and iPad media through `libimobiledevice` (the
//! `usbmuxd` daemon must be running). The root is the media area (DCIM, Downloads, Books…),
//! the only part AFC exposes without a jailbreak. Pairing and lock states are reported as
//! typed errors so the sidebar can say "Tap Trust on the device" or "Unlock the device".
//!
//! The `extern "C"` block mirrors `libimobiledevice/{libimobiledevice,lockdown,afc}.h` (1.3).
//! First build on Omarchy: check the `LOCKDOWN_E_*` values against the header.

use kiki_plugin_sdk::json::Value;
use kiki_plugin_sdk::{self as sdk, Describe, Entry, Features, Handler, Kind, Meta, Outgoing, PluginError, Result, WriteArgs};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::io::{Read, Write};
use std::sync::Mutex;

// ---------------------------------------------------------------- libimobiledevice FFI

type IDevice = c_void;
type LockdowndClient = c_void;
type AfcClient = c_void;

#[repr(C)]
struct LockdowndServiceDescriptor {
    port: u16,
    ssl_enabled: u8,
    identifier: *mut c_char,
}

const AFC_E_SUCCESS: c_int = 0;
const AFC_E_OBJECT_NOT_FOUND: c_int = 8;
const AFC_E_OBJECT_IS_DIR: c_int = 9;
const AFC_E_PERM_DENIED: c_int = 10;
const AFC_E_DIR_NOT_EMPTY: c_int = 19;
const AFC_FOPEN_RDONLY: c_int = 1;
const AFC_FOPEN_WRONLY: c_int = 3;

const LOCKDOWN_E_SUCCESS: c_int = 0;
const LOCKDOWN_E_PASSWORD_PROTECTED: c_int = -17;
const LOCKDOWN_E_USER_DENIED_PAIRING: c_int = -18;
const LOCKDOWN_E_PAIRING_DIALOG_RESPONSE_PENDING: c_int = -19;

const IDEVICE_E_SUCCESS: c_int = 0;

#[link(name = "imobiledevice-1.0")]
extern "C" {
    fn idevice_new(device: *mut *mut IDevice, udid: *const c_char) -> c_int;
    fn idevice_free(device: *mut IDevice) -> c_int;
    fn lockdownd_client_new_with_handshake(device: *mut IDevice, client: *mut *mut LockdowndClient, label: *const c_char) -> c_int;
    fn lockdownd_client_free(client: *mut LockdowndClient) -> c_int;
    fn lockdownd_start_service(client: *mut LockdowndClient, identifier: *const c_char, service: *mut *mut LockdowndServiceDescriptor) -> c_int;
    fn lockdownd_service_descriptor_free(service: *mut LockdowndServiceDescriptor) -> c_int;
    fn afc_client_new(device: *mut IDevice, service: *mut LockdowndServiceDescriptor, client: *mut *mut AfcClient) -> c_int;
    fn afc_client_free(client: *mut AfcClient) -> c_int;
    fn afc_read_directory(client: *mut AfcClient, path: *const c_char, list: *mut *mut *mut c_char) -> c_int;
    fn afc_get_file_info(client: *mut AfcClient, path: *const c_char, info: *mut *mut *mut c_char) -> c_int;
    fn afc_dictionary_free(dictionary: *mut *mut c_char) -> c_int;
    fn afc_file_open(client: *mut AfcClient, filename: *const c_char, mode: c_int, handle: *mut u64) -> c_int;
    fn afc_file_close(client: *mut AfcClient, handle: u64) -> c_int;
    fn afc_file_read(client: *mut AfcClient, handle: u64, data: *mut c_char, length: u32, bytes_read: *mut u32) -> c_int;
    fn afc_file_write(client: *mut AfcClient, handle: u64, data: *const c_char, length: u32, bytes_written: *mut u32) -> c_int;
    fn afc_file_seek(client: *mut AfcClient, handle: u64, offset: i64, whence: c_int) -> c_int;
    fn afc_remove_path(client: *mut AfcClient, path: *const c_char) -> c_int;
    fn afc_rename_path(client: *mut AfcClient, from: *const c_char, to: *const c_char) -> c_int;
    fn afc_make_directory(client: *mut AfcClient, path: *const c_char) -> c_int;
}

fn afc_err(code: c_int) -> PluginError {
    match code {
        AFC_E_OBJECT_NOT_FOUND => PluginError::not_found(),
        AFC_E_PERM_DENIED => PluginError::new("Denied", "the device refused (outside the media area?)"),
        AFC_E_DIR_NOT_EMPTY => PluginError::new("NotEmpty", "directory not empty"),
        AFC_E_OBJECT_IS_DIR => PluginError::io("is a directory"),
        c => PluginError::network(format!("AFC error {c}")),
    }
}

fn c(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| PluginError::invalid("path", "bad path"))
}

/// A NULL-terminated char** into owned strings, then freed.
fn take_list(list: *mut *mut c_char) -> Vec<String> {
    let mut out = Vec::new();
    if list.is_null() {
        return out;
    }
    unsafe {
        let mut p = list;
        while !(*p).is_null() {
            out.push(CStr::from_ptr(*p).to_string_lossy().into_owned());
            p = p.add(1);
        }
        afc_dictionary_free(list);
    }
    out
}

// ---------------------------------------------------------------- session

struct Open {
    dev: *mut IDevice,
    afc: *mut AfcClient,
}
unsafe impl Send for Open {}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe {
            afc_client_free(self.afc);
            idevice_free(self.dev);
        }
    }
}

struct Afc {
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

/// (kind, size, mtime_ms) from the key/value info list
fn parse_info(kv: &[String]) -> (Kind, u64, u64) {
    let mut kind = Kind::File;
    let mut size = 0;
    let mut mtime = 0;
    for pair in kv.chunks(2) {
        if pair.len() < 2 {
            break;
        }
        match pair[0].as_str() {
            "st_ifmt" => {
                kind = match pair[1].as_str() {
                    "S_IFDIR" => Kind::Dir,
                    "S_IFLNK" => Kind::Link,
                    "S_IFREG" => Kind::File,
                    _ => Kind::Other,
                }
            }
            "st_size" => size = pair[1].parse().unwrap_or(0),
            "st_mtime" => mtime = pair[1].parse::<u64>().unwrap_or(0) / 1_000_000, // nanoseconds
            _ => {}
        }
    }
    (kind, size, mtime)
}

impl Afc {
    fn with<T>(&self, f: impl FnOnce(&Open) -> Result<T>) -> Result<T> {
        let g = self.session.lock().unwrap();
        let s = g.as_ref().ok_or_else(|| PluginError::network("not connected"))?;
        f(s)
    }

    fn info(s: &Open, path: &str) -> Result<(Kind, u64, u64)> {
        let cp = c(path)?;
        let mut list: *mut *mut c_char = std::ptr::null_mut();
        let r = unsafe { afc_get_file_info(s.afc, cp.as_ptr(), &mut list) };
        if r != AFC_E_SUCCESS {
            return Err(afc_err(r));
        }
        Ok(parse_info(&take_list(list)))
    }
}

fn open_device(udid: &str) -> Result<Open> {
    unsafe {
        let mut dev: *mut IDevice = std::ptr::null_mut();
        let cu = if udid.is_empty() { None } else { Some(c(udid)?) };
        let r = idevice_new(&mut dev, cu.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null()));
        if r != IDEVICE_E_SUCCESS || dev.is_null() {
            return Err(PluginError::network("no iOS device found; is usbmuxd running?"));
        }
        let mut ld: *mut LockdowndClient = std::ptr::null_mut();
        let label = CString::new("kiki").unwrap();
        let r = lockdownd_client_new_with_handshake(dev, &mut ld, label.as_ptr());
        if r != LOCKDOWN_E_SUCCESS {
            idevice_free(dev);
            return Err(match r {
                LOCKDOWN_E_PASSWORD_PROTECTED => PluginError::auth("Unlock the device, then retry"),
                LOCKDOWN_E_PAIRING_DIALOG_RESPONSE_PENDING => PluginError::invalid("trust", "Tap Trust on the device, then retry"),
                LOCKDOWN_E_USER_DENIED_PAIRING => PluginError::invalid("trust", "The device refused pairing; unplug it and tap Trust next time"),
                c => PluginError::network(format!("lockdown handshake failed ({c})")),
            });
        }
        let svc_name = CString::new("com.apple.afc").unwrap();
        let mut svc: *mut LockdowndServiceDescriptor = std::ptr::null_mut();
        let r = lockdownd_start_service(ld, svc_name.as_ptr(), &mut svc);
        if r != LOCKDOWN_E_SUCCESS || svc.is_null() {
            lockdownd_client_free(ld);
            idevice_free(dev);
            return Err(PluginError::network(format!("cannot start the AFC service ({r})")));
        }
        let mut afc: *mut AfcClient = std::ptr::null_mut();
        let r = afc_client_new(dev, svc, &mut afc);
        lockdownd_service_descriptor_free(svc);
        lockdownd_client_free(ld);
        if r != AFC_E_SUCCESS || afc.is_null() {
            idevice_free(dev);
            return Err(afc_err(r));
        }
        Ok(Open { dev, afc })
    }
}

impl Handler for Afc {
    fn describe(&self) -> Describe {
        Describe {
            scheme: "afc",
            display_name: "iPhone / iPad (AFC)",
            version: "0.1",
            form: vec![sdk::field("serial", "UDID", "text", false, None)],
            defaults: Value::obj().done(),
            secret_fields: vec![],
            detector_upload: "sizeMtime",
            detector_download: "sizeMtime",
            features: Features { set_mtime: false, mode: false, real_dirs: true, meta_in_scan: true, pipelining: false, partial_read: true },
            available: None,
        }
    }

    fn validate(&self, _config: &Value) -> Result<()> {
        Ok(())
    }

    fn connect(&self, _location: &str, _role: &str, config: &Value, _secrets: &Value) -> Result<Value> {
        let mut g = self.session.lock().unwrap();
        if g.is_none() {
            *g = Some(open_device(config.str_field("serial").unwrap_or(""))?);
        }
        Ok(Value::obj().v("fingerprint", Value::Null).s("banner", "media area only (DCIM, Downloads, Books)").done())
    }

    fn disconnect(&self, _location: &str, role: &str) {
        if role == "browse" {
            *self.session.lock().unwrap() = None;
        }
    }

    fn capabilities(&self, _location: &str) -> Result<Value> {
        Ok(Value::obj().b("trash", false).b("setMtime", false).b("mode", false).b("realDirs", true).v("digestKind", Value::Null).s("separator", "/").s("fastScan", "none").b("partialRead", true).done())
    }

    fn scan(&self, _location: &str, path: &str, recursive: bool, sink: &mut dyn FnMut(Vec<Entry>)) -> Result<u64> {
        if recursive {
            return Err(PluginError::unsupported());
        }
        self.with(|s| {
            let dir = norm(path);
            let cp = c(&dir)?;
            let mut list: *mut *mut c_char = std::ptr::null_mut();
            let r = unsafe { afc_read_directory(s.afc, cp.as_ptr(), &mut list) };
            if r != AFC_E_SUCCESS {
                return Err(afc_err(r));
            }
            let mut entries = Vec::new();
            for name in take_list(list) {
                if name == "." || name == ".." {
                    continue;
                }
                if sdk::cancelled() {
                    return Err(sdk::cancel_error());
                }
                let full = if dir == "/" { format!("/{name}") } else { format!("{dir}/{name}") };
                let (kind, size, mtime) = Afc::info(s, &full).unwrap_or((Kind::Other, 0, 0));
                entries.push(Entry { name, kind, meta: Some(Meta { hidden: false, size, mtime_ms: mtime, mode: None, owner: None, group: None }), rel: String::new() });
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
        self.with(|s| {
            let (_, size, mtime) = Afc::info(s, &norm(path))?;
            Ok(Meta { hidden: false, size, mtime_ms: mtime, mode: None, owner: None, group: None })
        })
    }

    fn read(&self, _location: &str, path: &str, offset: u64, out: &mut Outgoing) -> Result<()> {
        self.with(|s| {
            let cp = c(&norm(path))?;
            let mut h: u64 = 0;
            let r = unsafe { afc_file_open(s.afc, cp.as_ptr(), AFC_FOPEN_RDONLY, &mut h) };
            if r != AFC_E_SUCCESS {
                return Err(afc_err(r));
            }
            let result = (|| {
                if offset > 0 {
                    let r = unsafe { afc_file_seek(s.afc, h, offset as i64, 0) };
                    if r != AFC_E_SUCCESS {
                        return Err(afc_err(r));
                    }
                }
                let mut buf = vec![0u8; 256 * 1024];
                loop {
                    if sdk::cancelled() {
                        return Err(sdk::cancel_error());
                    }
                    let mut got: u32 = 0;
                    let r = unsafe { afc_file_read(s.afc, h, buf.as_mut_ptr().cast::<c_char>(), buf.len() as u32, &mut got) };
                    if r != AFC_E_SUCCESS {
                        return Err(afc_err(r));
                    }
                    if got == 0 {
                        break;
                    }
                    out.write_all(&buf[..got as usize]).map_err(PluginError::io)?;
                }
                Ok(())
            })();
            unsafe { afc_file_close(s.afc, h) };
            result
        })
    }

    fn write(&self, _location: &str, path: &str, mut args: WriteArgs) -> Result<u64> {
        self.with(|s| {
            let cp = c(&norm(path))?;
            let mut h: u64 = 0;
            let r = unsafe { afc_file_open(s.afc, cp.as_ptr(), AFC_FOPEN_WRONLY, &mut h) };
            if r != AFC_E_SUCCESS {
                return Err(afc_err(r));
            }
            let mut total = 0u64;
            let result = (|| {
                let mut buf = vec![0u8; 256 * 1024];
                loop {
                    if sdk::cancelled() {
                        return Err(sdk::cancel_error());
                    }
                    let n = args.data.read(&mut buf).map_err(PluginError::io)?;
                    if n == 0 {
                        break;
                    }
                    let mut written: u32 = 0;
                    let r = unsafe { afc_file_write(s.afc, h, buf.as_ptr().cast::<c_char>(), n as u32, &mut written) };
                    if r != AFC_E_SUCCESS {
                        return Err(afc_err(r));
                    }
                    total += written as u64;
                }
                Ok(())
            })();
            unsafe { afc_file_close(s.afc, h) };
            result.map(|_| total)
        })
    }

    fn mkdir(&self, _location: &str, path: &str) -> Result<()> {
        self.with(|s| {
            let cp = c(&norm(path))?;
            let r = unsafe { afc_make_directory(s.afc, cp.as_ptr()) };
            if r != AFC_E_SUCCESS {
                return Err(afc_err(r));
            }
            Ok(())
        })
    }

    fn rename(&self, _location: &str, from: &str, to: &str) -> Result<()> {
        self.with(|s| {
            let (cf, ct) = (c(&norm(from))?, c(&norm(to))?);
            let r = unsafe { afc_rename_path(s.afc, cf.as_ptr(), ct.as_ptr()) };
            if r != AFC_E_SUCCESS {
                return Err(afc_err(r));
            }
            Ok(())
        })
    }

    fn delete(&self, _location: &str, path: &str) -> Result<()> {
        self.with(|s| {
            let cp = c(&norm(path))?;
            let r = unsafe { afc_remove_path(s.afc, cp.as_ptr()) };
            if r != AFC_E_SUCCESS {
                return Err(afc_err(r));
            }
            Ok(())
        })
    }
}

fn main() {
    let h = Afc { session: Mutex::new(None) };
    if let Err(e) = sdk::run(&h) {
        eprintln!("kiki-plugin-afc: {e}");
    }
}
