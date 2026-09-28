//! Which Masonry widget is currently each field the app can name.
//!
//! Fields that can be focused by a key register their text area's
//! `WidgetId` here under an [`app::focus::FieldKey`]; the key map looks it
//! up when the key arrives. The app has one window, so one registry.

use std::{
  collections::HashMap,
  sync::{LazyLock, Mutex, MutexGuard},
};

use app::focus::FieldKey as Key;
pub use app::focus::{FieldKey, FocusRequests};
use masonry::core::WidgetId;

static FIELDS: LazyLock<Mutex<HashMap<Key, WidgetId>>> =
  LazyLock::new(Default::default);

fn fields() -> MutexGuard<'static, HashMap<Key, WidgetId>> {
  FIELDS.lock().expect("focus registry poisoned")
}

/// Record that `key` is the text area `id`.
pub fn register(key: Key, id: WidgetId) { fields().insert(key, id); }

/// Forget `key`, if it still names `id`. A field being torn down after its
/// replacement registered leaves the replacement in place.
pub fn unregister(key: Key, id: WidgetId) {
  let mut fields = fields();
  if fields.get(&key) == Some(&id) {
    fields.remove(&key);
  }
}

/// The text area registered under `key`, if one is mounted.
pub fn lookup(key: Key) -> Option<WidgetId> { fields().get(&key).copied() }
