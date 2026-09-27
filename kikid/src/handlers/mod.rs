//! The daemon's verbs, by area (docs/0.3.0/01-daemon-on-demand.md, decision 6).
//!
//! `server.rs` owns the connection and the wire — reading, writing, `Hello`, the numbered
//! errors — and asks each module here in turn whether a request is its. A module's `handle`
//! answers `None` for a verb it does not own and `Some(reply)` for one it does; the first to
//! answer wins, and a verb nobody takes is refused as unknown. The modules are the areas of the
//! daemon a person would name — listings, jobs, locations, Quick Look — so that a verb is found
//! by asking what it is about, and a new one is written once, in its area, rather than into a
//! match a thousand lines long.
//!
//! **A connection answers its requests one after another, in the order they arrive.** That is
//! what keeps a window's requests coherent: a `Sort` sent after an `Open` is applied to the
//! listing the `Open` made, and a `Close` never races the `Window` before it. It is also why a
//! handler must not be slow. **A handler that can take longer than a listing answers through
//! `cx.later`**: the request's thread returns at once, the work runs on a thread of its own, and
//! the reply — with its number, if the work said one — goes out from there. The rule is written
//! at `later`, once, so that nobody hand-rolls a thread and gets the numbered reply wrong, and
//! nobody discovers the ordering rule again by a pane that waits behind a PDF render.

use crate::json::Value;
use crate::server::Cx;

pub mod ai;
pub mod devices;
pub mod git;
pub mod index;
pub mod integrate;
pub mod jobs;
pub mod listing;
pub mod locations;
pub mod mirror;
pub mod open;
pub mod quicklook;
pub mod session;
pub mod settings;
pub mod share;
pub mod tree;

/// What a handler answers: a value to reply with now, `None` for a reply that will be sent
/// later by whoever holds the work (`cx.later`, a job, a listing's sort), or the error — its
/// code and message; a number, if one was said, rides beside them (`server::vfs_err`).
pub type Reply = Result<Option<Value>, (&'static str, String)>;

/// A module's entry: `None` when the verb is not its.
pub type Handler = fn(&mut Cx, &str, &Value) -> Option<Reply>;

/// Every area, in the order they are asked. The order does not matter for correctness — no verb
/// belongs to two modules — only for where a reader looks first.
pub const ALL: &[Handler] = &[
    session::handle,
    listing::handle,
    tree::handle,
    jobs::handle,
    locations::handle,
    devices::handle,
    open::handle,
    quicklook::handle,
    settings::handle,
    integrate::handle,
    share::handle,
    ai::handle,
    mirror::handle,
    git::handle,
    index::handle,
];
