//! Who this machine is to a LocalSend receiver: a self-signed certificate, made once and kept.
//!
//! Receivers ask the sender for a certificate during the TLS handshake (mutual TLS) and refuse
//! the connection without one — "certificate required". They name the sender by that
//! certificate's SHA-256, which is also the `fingerprint` the sender must announce. There is no
//! CA on either side: the certificate is the identity.

use kiki_plugin_sdk::{PluginError, Result};
use std::path::PathBuf;

pub struct Identity {
    pub cert_der: Vec<u8>,
    /// PKCS#8.
    pub key_der: Vec<u8>,
}

impl Identity {
    /// SHA-256 of the certificate, lower-case hex: what the protocol calls the fingerprint.
    pub fn fingerprint(&self) -> String {
        ring::digest::digest(&ring::digest::SHA256, &self.cert_der).as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// The plugin's own corner of the cache, the only place the share contract lets it write.
fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"));
    base.join("kiki-share-localsend")
}

fn generate() -> Result<Identity> {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).map_err(|e| PluginError::io(e.to_string()))?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    params.distinguished_name.push(rcgen::DnType::CommonName, "kiki");
    let key = rcgen::KeyPair::generate().map_err(|e| PluginError::io(e.to_string()))?;
    let cert = params.self_signed(&key).map_err(|e| PluginError::io(e.to_string()))?;
    Ok(Identity { cert_der: cert.der().to_vec(), key_der: key.serialize_der() })
}

/// The kept identity, or a new one. Kept so that a receiver which has seen this machine before
/// (a favourite, "always accept from") sees the same one again. One that cannot be kept — a
/// read-only cache — still works for this send.
pub fn load_or_create() -> Result<Identity> {
    let (cert_path, key_path) = (dir().join("identity.crt.der"), dir().join("identity.key.der"));
    if let (Ok(cert_der), Ok(key_der)) = (std::fs::read(&cert_path), std::fs::read(&key_path)) {
        if !cert_der.is_empty() && !key_der.is_empty() {
            return Ok(Identity { cert_der, key_der });
        }
    }
    let id = generate()?;
    if std::fs::create_dir_all(dir()).is_ok() {
        use std::os::unix::fs::OpenOptionsExt;
        let write = |p: &PathBuf, bytes: &[u8]| -> std::io::Result<()> {
            let _ = std::fs::remove_file(p);
            let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(p)?;
            std::io::Write::write_all(&mut f, bytes)
        };
        // Both or neither: a certificate without its key is an identity that cannot be used.
        if write(&key_path, &id.key_der).and_then(|_| write(&cert_path, &id.cert_der)).is_err() {
            let _ = std::fs::remove_file(&key_path);
            let _ = std::fs::remove_file(&cert_path);
        }
    }
    Ok(id)
}

/// Loaded once per process: a send makes one connection per file.
pub fn get() -> Result<&'static Identity> {
    static ME: std::sync::OnceLock<std::result::Result<Identity, String>> = std::sync::OnceLock::new();
    ME.get_or_init(|| load_or_create().map_err(|e| e.message.clone())).as_ref().map_err(|e| PluginError::io(e.clone()))
}
