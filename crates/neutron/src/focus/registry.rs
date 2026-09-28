//! Which Masonry widget is currently each named field.
//!
//! Fields that can be focused by a key register their text area's
//! `WidgetId` here under a [`FieldKey`]; the key map looks it up when the
//! key arrives. The app has one window, so one registry.

use std::{
  collections::HashMap,
  sync::{LazyLock, Mutex, MutexGuard},
};

use masonry::core::WidgetId;

use super::FieldKey;

static FIELDS: LazyLock<Mutex<HashMap<FieldKey, WidgetId>>> =
  LazyLock::new(Default::default);

fn fields() -> MutexGuard<'static, HashMap<FieldKey, WidgetId>> {
  FIELDS.lock().expect("focus registry poisoned")
}

/// Record that `key` is the text area `id`.
pub fn register(key: FieldKey, id: WidgetId) { fields().insert(key, id); }

/// Forget `key`, if it still names `id`. A field being torn down after its
/// replacement registered leaves the replacement in place.
pub fn unregister(key: FieldKey, id: WidgetId) {
  let mut fields = fields();
  if fields.get(&key) == Some(&id) {
    fields.remove(&key);
  }
}

/// The text area registered under `key`, if one is mounted.
pub fn lookup(key: FieldKey) -> Option<WidgetId> { fields().get(&key).copied() }
