//! kikid as a library: the daemon's modules, so tests and the stub plugin can use them.

pub mod archive;
pub mod config;
pub mod jobs;
pub use kiki_json as json;
pub mod kinds;
pub mod listing;
pub mod md5;
pub mod ops;
pub mod preview;
pub mod proto;
pub mod server;
pub mod string_pool;
pub mod thumbs;
pub mod toml;
pub mod tree;
pub mod vfs;
pub mod watch;
pub mod git;
pub mod helpers;
pub mod index;
pub mod locations;
pub mod openin;
pub mod mirror;
pub mod plugin;

/// Tests that set process-wide environment variables take this lock so they never interleave.
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
