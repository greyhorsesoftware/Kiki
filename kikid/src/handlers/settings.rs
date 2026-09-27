//! What the person has set: `Settings`, `SetSettings`, the per-folder `ViewPrefs`,
//! `SetViewPref`, `ClearViewPrefs`, the sidebar's `Favorites` and `SetFavorites`, and
//! `TrashInfo`. A change that other windows show is broadcast, not just answered.

use super::Reply;
use crate::json::Value;
use crate::proto;
use crate::server::Cx;

pub fn handle(_cx: &mut Cx, name: &str, b: &Value) -> Option<Reply> {
    Some(match name {
        "Favorites" => Ok(Some(Value::obj().v("items", crate::config::favorites()).done())),
        "SetFavorites" => match b.get("items").and_then(Value::as_arr) {
            // Every window, not just the one that asked: a second window's sidebar listens
            // for this, and until now nobody ever said it.
            Some(items) => crate::config::set_favorites(items)
                .map(|_| {
                    crate::jobs::broadcast(proto::event("FavoritesChanged").done());
                    Some(Value::obj().done())
                })
                .map_err(|e| ("Io", e.to_string())),
            None => Err(("Protocol", "missing items".into())),
        },
        "TrashInfo" => Ok(Some(Value::obj().v("items", Value::Arr(crate::ops::trash_infos().into_iter().map(|(n, p, d)| Value::obj().s("name", n).s("path", p).s("deleted", d).done()).collect())).done())),
        "Settings" => Ok(Some(crate::config::settings())),
        "ViewPrefs" => Ok(Some(Value::obj().v("folders", crate::config::view_prefs()).done())),
        "SetViewPref" => {
            let uri = b.str_field("uri").unwrap_or("");
            if uri.is_empty() {
                Err(("Protocol", "missing uri".into()))
            } else {
                crate::config::set_view_pref(uri, b.str_field("view").unwrap_or("list"), b.str_field("sort").unwrap_or("name"), b.str_field("order").unwrap_or("asc"), b.get("hidden").and_then(Value::as_bool))
                    .map(|_| {
                        crate::jobs::broadcast(proto::event("ViewPrefsChanged").s("uri", uri).done());
                        Some(Value::obj().done())
                    })
                    .map_err(|e| ("Io", e.to_string()))
            }
        }
        "ClearViewPrefs" => crate::config::clear_view_prefs()
            .map(|_| {
                crate::jobs::broadcast(proto::event("ViewPrefsChanged").done());
                Some(Value::obj().done())
            })
            .map_err(|e| ("Io", e.to_string())),
        "SetSettings" => match b.get("patch") {
            Some(p) => crate::config::set_settings(p).map(|_| Some(Value::obj().done())).map_err(|e| ("Io", e.to_string())),
            None => Err(("Protocol", "missing patch".into())),
        },
        _ => return None,
    })
}
