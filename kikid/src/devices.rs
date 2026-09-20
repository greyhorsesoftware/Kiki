//! Devices (plan 17): phones and cameras on USB as transient locations. The daemon classifies
//! USB interfaces from sysfs (PTP, MTP, Apple/AFC), keeps a registry that the sidebar's Devices
//! section mirrors, and hands each device to its protocol plugin as a location whose config
//! carries the bus address and serial. Nothing here links a device library.
//!
//! Hotplug: `/sys/bus/usb/devices` is rescanned every two seconds (a change to `/dev/bus/usb`
//! is what the kernel emits on plug and unplug; polling sysfs keeps this free of netlink).

use crate::json::Value;
use crate::vfs::VfsError;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// `ptp` | `mtp` | `afc`
    pub kind: &'static str,
    /// URI authority: `<vendor>-<model>-<serial>` slug
    pub authority: String,
    pub vendor: String,
    pub model: String,
    pub serial: String,
    pub bus: u32,
    pub devnum: u32,
    pub sysfs: PathBuf,
    /// Another process (gvfs, kio) already holds the device
    pub busy: Option<String>,
}

impl Device {
    pub fn uri(&self) -> String {
        format!("{}://{}/", self.kind, self.authority)
    }

    pub fn display_name(&self) -> String {
        names().get(&self.authority).cloned().unwrap_or_else(|| if self.model.is_empty() { format!("{} {}", self.vendor, self.kind.to_uppercase()) } else { self.model.clone() })
    }

    pub fn to_json(&self, connected: bool) -> Value {
        Value::obj()
            .s("uri", self.uri())
            .s("kind", self.kind)
            .s("name", self.display_name())
            .s("vendor", self.vendor.clone())
            .s("model", self.model.clone())
            .s("serial", self.serial.clone())
            .b("connected", connected)
            .opt_s("busy", self.busy.as_deref())
            .done()
    }

    /// The transient location the resolver hands to `locations::connect`.
    pub fn location(&self) -> Value {
        Value::obj()
            .s("name", self.authority.clone())
            .s("plugin", self.kind)
            .s("remoteUri", self.uri())
            .s("localUri", "")
            .b("transient", true)
            .v(
                "config",
                Value::obj()
                    .s("serial", self.serial.clone())
                    .s("vendor", self.vendor.clone())
                    .s("model", self.model.clone())
                    .u("bus", self.bus as u64)
                    .u("devnum", self.devnum as u64)
                    .s("sysfs", self.sysfs.to_string_lossy().into_owned())
                    .done(),
            )
            .done()
    }
}

struct Registry {
    devices: Vec<Device>,
    /// ejected until unplugged: authority
    ejected: Vec<String>,
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Registry { devices: Vec::new(), ejected: Vec::new() }))
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

fn read_attr(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Classify one USB device directory (`/sys/bus/usb/devices/1-3`) from its interfaces.
pub fn classify(dev_dir: &Path) -> Option<Device> {
    let vid = read_attr(dev_dir, "idVendor");
    if vid.is_empty() || dev_dir.join("bInterfaceClass").exists() {
        return None; // an interface dir, or not a device
    }
    let vendor = read_attr(dev_dir, "manufacturer");
    let model = read_attr(dev_dir, "product");
    let serial = read_attr(dev_dir, "serial");
    let bus: u32 = read_attr(dev_dir, "busnum").parse().unwrap_or(0);
    let devnum: u32 = read_attr(dev_dir, "devnum").parse().unwrap_or(0);
    let mut kind: Option<&'static str> = None;
    if vid.eq_ignore_ascii_case("05ac") && (model.contains("iPhone") || model.contains("iPad") || model.contains("iPod")) {
        kind = Some("afc");
    } else if let Ok(rd) = std::fs::read_dir(dev_dir) {
        let base = dev_dir.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with(&format!("{base}:")) {
                continue;
            }
            let ifc = e.path();
            let class = read_attr(&ifc, "bInterfaceClass");
            let sub = read_attr(&ifc, "bInterfaceSubClass");
            let proto = read_attr(&ifc, "bInterfaceProtocol");
            let label = read_attr(&ifc, "interface").to_ascii_uppercase();
            if label.contains("MTP") || (class == "ff" && sub == "ff" && proto == "00" && label.contains("MTP")) {
                kind = Some("mtp");
                break;
            }
            if class == "06" && sub == "01" {
                // PTP class; Android in file-transfer mode also exposes this with an MTP label
                kind = Some(if label.contains("MTP") { "mtp" } else { "ptp" });
            }
        }
    }
    let kind = kind?;
    let authority = slug(&format!("{}-{}-{}", if vendor.is_empty() { vid.as_str() } else { vendor.as_str() }, model, if serial.is_empty() { format!("{bus}-{devnum}") } else { serial.clone() }));
    let busy = busy_by(bus, devnum);
    Some(Device { kind, authority, vendor, model, serial, bus, devnum, sysfs: dev_dir.to_path_buf(), busy })
}

/// Another process holding the device node open (gvfs-mtp, kiod) makes the device unusable.
fn busy_by(bus: u32, devnum: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let node = format!("/dev/bus/usb/{bus:03}/{devnum:03}");
        if let Ok(o) = std::process::Command::new("fuser").arg(&node).stderr(std::process::Stdio::null()).output() {
            let pids: Vec<&str> = std::str::from_utf8(&o.stdout).unwrap_or("").split_whitespace().collect();
            for pid in pids {
                let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid.trim_matches(|c: char| !c.is_ascii_digit()))).unwrap_or_default();
                let comm = comm.trim();
                if !comm.is_empty() && comm != "kikid" && !comm.starts_with("kiki-plugin") {
                    return Some(comm.to_string());
                }
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (bus, devnum);
        None
    }
}

pub fn scan_sysfs(root: &Path) -> Vec<Device> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("usb") || name.contains(':') {
                continue;
            }
            if let Some(d) = classify(&e.path()) {
                out.push(d);
            }
        }
    }
    out.sort_by(|a, b| a.authority.cmp(&b.authority));
    out
}

fn sysfs_root() -> PathBuf {
    std::env::var("KIKI_SYSFS_USB").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/sys/bus/usb/devices"))
}

/// The kinds a device can be. A build that ships none of them never looks for devices.
const KINDS: &[&str] = &["ptp", "mtp", "afc"];

/// Only devices this build can open are devices at all: one whose kind does not ship would sit
/// in the sidebar and fail with `Unsupported` when clicked (plan 31, phase 1).
fn openable(found: Vec<Device>, ships: impl Fn(&str) -> bool) -> Vec<Device> {
    found.into_iter().filter(|d| ships(d.kind)).collect()
}

/// One pass: rescan, diff against the registry, broadcast changes.
pub fn refresh() {
    let now = openable(scan_sysfs(&sysfs_root()), crate::plugin::ships);
    let (added, removed) = {
        let mut r = registry().lock().unwrap();
        r.ejected.retain(|a| now.iter().any(|d| &d.authority == a));
        let added: Vec<Device> = now.iter().filter(|d| !r.devices.iter().any(|o| o.authority == d.authority)).cloned().collect();
        let removed: Vec<Device> = r.devices.iter().filter(|o| !now.iter().any(|d| d.authority == o.authority)).cloned().collect();
        r.devices = now;
        (added, removed)
    };
    for d in removed {
        crate::locations::disconnect(&d.authority);
        crate::jobs::broadcast(crate::proto::event("DeviceRemoved").s("uri", d.uri()).done());
    }
    for d in added {
        remember(&d);
        crate::jobs::broadcast(crate::proto::event("DeviceAdded").v("device", d.to_json(false)).done());
    }
}

pub fn start() {
    if !KINDS.iter().any(|k| crate::plugin::ships(k)) {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        std::thread::Builder::new()
            .name("devices".into())
            .spawn(|| loop {
                refresh();
                std::thread::sleep(Duration::from_secs(2));
            })
            .expect("spawn devices");
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = Duration::from_secs(2);
    }
}

pub fn list_json() -> Value {
    let r = registry().lock().unwrap();
    let sessions = crate::locations::connected_names();
    Value::obj().v("devices", Value::Arr(r.devices.iter().filter(|d| !r.ejected.contains(&d.authority)).map(|d| d.to_json(sessions.contains(&d.authority))).collect())).done()
}

/// The transient location for a device URI, if that device is present and not ejected.
pub fn location_for(scheme: &str, authority: &str) -> Option<Value> {
    if !matches!(scheme, "ptp" | "mtp" | "afc") {
        return None;
    }
    let r = registry().lock().unwrap();
    if r.ejected.iter().any(|a| a == authority) {
        return None;
    }
    r.devices.iter().find(|d| d.kind == scheme && d.authority == authority).map(Device::location)
}

/// Eject: close the plugin session (the plugin sends the protocol's close) and hide the device
/// until it is unplugged and plugged again.
pub fn eject(uri: &str) -> Result<(), VfsError> {
    let u = crate::vfs::uri::Uri::parse(uri).map_err(|e| VfsError::Io(e.0.to_string()))?;
    let mut r = registry().lock().unwrap();
    if !r.devices.iter().any(|d| d.kind == u.scheme && d.authority == u.authority) {
        return Err(VfsError::NotFound);
    }
    if !r.ejected.contains(&u.authority) {
        r.ejected.push(u.authority.clone());
    }
    drop(r);
    crate::locations::disconnect(&u.authority);
    crate::jobs::broadcast(crate::proto::event("DeviceRemoved").s("uri", uri.to_string()).done());
    Ok(())
}

// ---------------------------------------------------------------- devices.toml (names, last seen)

fn names() -> BTreeMap<String, String> {
    let v = crate::config::read_named("devices.toml");
    let mut out = BTreeMap::new();
    if let Some(Value::Arr(a)) = v.get("device") {
        for d in a {
            if let (Some(a), Some(n)) = (d.str_field("authority"), d.str_field("name")) {
                if !n.is_empty() {
                    out.insert(a.to_string(), n.to_string());
                }
            }
        }
    }
    out
}

fn remember(d: &Device) {
    let v = crate::config::read_named("devices.toml");
    let mut list: Vec<Value> = match v.get("device") {
        Some(Value::Arr(a)) => a.clone(),
        _ => Vec::new(),
    };
    let now = crate::ops::unix_now();
    if let Some(e) = list.iter_mut().find(|e| e.str_field("authority") == Some(d.authority.as_str())) {
        if let Value::Obj(m) = e {
            m.insert("lastSeen".into(), Value::Uint(now));
        }
    } else {
        list.push(Value::obj().s("authority", d.authority.clone()).s("name", "").s("kind", d.kind).u("lastSeen", now).done());
    }
    let mut m = BTreeMap::new();
    m.insert("device".to_string(), Value::Arr(list));
    let _ = crate::config::write_named("devices.toml", &Value::Obj(m));
}

pub fn rename(authority: &str, name: &str) -> std::io::Result<()> {
    let v = crate::config::read_named("devices.toml");
    let mut list: Vec<Value> = match v.get("device") {
        Some(Value::Arr(a)) => a.clone(),
        _ => Vec::new(),
    };
    match list.iter_mut().find(|e| e.str_field("authority") == Some(authority)) {
        Some(Value::Obj(m)) => {
            m.insert("name".into(), Value::Str(name.to_string()));
        }
        _ => list.push(Value::obj().s("authority", authority).s("name", name).done()),
    }
    let mut m = BTreeMap::new();
    m.insert("device".to_string(), Value::Arr(list));
    crate::config::write_named("devices.toml", &Value::Obj(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usb(root: &Path, id: &str, attrs: &[(&str, &str)], interfaces: &[(&str, &[(&str, &str)])]) {
        let d = root.join(id);
        std::fs::create_dir_all(&d).unwrap();
        for (k, v) in attrs {
            std::fs::write(d.join(k), format!("{v}\n")).unwrap();
        }
        for (n, ia) in interfaces {
            let i = root.join(format!("{id}:{n}"));
            std::fs::create_dir_all(&i).unwrap();
            let inner = d.join(format!("{id}:{n}"));
            std::fs::create_dir_all(&inner).unwrap();
            for (k, v) in *ia {
                std::fs::write(i.join(k), format!("{v}\n")).unwrap();
                std::fs::write(inner.join(k), format!("{v}\n")).unwrap();
            }
        }
    }

    #[test]
    fn classifies_camera_phone_and_iphone_and_ignores_the_rest() {
        let root = std::env::temp_dir().join(format!("kiki-sysfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        usb(
            &root,
            "1-2",
            &[("idVendor", "04a9"), ("manufacturer", "Canon Inc."), ("product", "Canon EOS R6"), ("serial", "ABC123"), ("busnum", "1"), ("devnum", "4")],
            &[("1.0", &[("bInterfaceClass", "06"), ("bInterfaceSubClass", "01"), ("bInterfaceProtocol", "01")])],
        );
        usb(
            &root,
            "1-3",
            &[("idVendor", "18d1"), ("manufacturer", "Google"), ("product", "Pixel 8"), ("serial", "ZX9"), ("busnum", "1"), ("devnum", "5")],
            &[("1.0", &[("bInterfaceClass", "06"), ("bInterfaceSubClass", "01"), ("bInterfaceProtocol", "01"), ("interface", "MTP")])],
        );
        usb(
            &root,
            "1-4",
            &[("idVendor", "05ac"), ("manufacturer", "Apple Inc."), ("product", "iPhone"), ("serial", "00008030-000A"), ("busnum", "1"), ("devnum", "6")],
            &[("1.0", &[("bInterfaceClass", "06"), ("bInterfaceSubClass", "01"), ("bInterfaceProtocol", "01")])],
        );
        usb(
            &root,
            "1-5",
            &[("idVendor", "046d"), ("manufacturer", "Logitech"), ("product", "USB Receiver"), ("busnum", "1"), ("devnum", "7")],
            &[("1.0", &[("bInterfaceClass", "03"), ("bInterfaceSubClass", "01"), ("bInterfaceProtocol", "02")])],
        );
        std::fs::create_dir_all(root.join("usb1")).unwrap();
        let devs = scan_sysfs(&root);
        let kinds: Vec<(&str, &str)> = devs.iter().map(|d| (d.kind, d.authority.as_str())).collect();
        assert_eq!(kinds, vec![("afc", "apple-inc-iphone-00008030-000a"), ("ptp", "canon-inc-canon-eos-r6-abc123"), ("mtp", "google-pixel-8-zx9")]);
        assert_eq!(devs[1].uri(), "ptp://canon-inc-canon-eos-r6-abc123/");
        let loc = devs[2].location();
        assert_eq!(loc.str_field("plugin"), Some("mtp"));
        assert_eq!(loc.get("config").unwrap().str_field("serial"), Some("ZX9"));
        // A kind this build does not ship is not a device: it could only fail when opened.
        let only_mtp: Vec<&str> = openable(devs.clone(), |k| k == "mtp").iter().map(|d| d.kind).collect();
        assert_eq!(only_mtp, vec!["mtp"]);
        assert!(openable(devs, |_| false).is_empty());
        assert!(!KINDS.iter().any(|k| crate::plugin::ships(k)), "0.1.0 ships no device kind (plan 31); update this when one does");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Canon Inc. / EOS R6 (ABC)"), "canon-inc-eos-r6-abc");
        assert_eq!(slug("  --x"), "x");
    }
}
