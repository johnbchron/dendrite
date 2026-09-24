//! Where keyboard focus can be sent by name.
//!
//! Masonry only lets a widget move focus while it is handling an event, and
//! the key map is the widget handling the keystroke that asks for a field
//! ("Enter to rename"). So fields that can be focused by a key register their
//! text area's `WidgetId` here under a [`FieldKey`], and the key map looks it
//! up when the key arrives. The app has one window, so one registry.

use std::{
  collections::HashMap,
  sync::{Arc, LazyLock, Mutex},
};

use masonry::core::WidgetId;

/// A text field the key map can focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldKey {
  /// The inspector's node name.
  Title,
  /// The inspector's requirement search, which arms link mode.
  LinkSearch,
  /// The command palette's search.
  PaletteSearch,
  /// The quest switcher's search.
  QuestSearch,
}

static FIELDS: LazyLock<Mutex<HashMap<FieldKey, WidgetId>>> =
  LazyLock::new(Default::default);

fn fields() -> std::sync::MutexGuard<'static, HashMap<FieldKey, WidgetId>> {
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

/// Fields the app has asked to focus once the views catch up: shared
/// between the app state, which files a request, and
/// [`crate::driver::FocusDriver`], which serves it after the next rebuild.
#[derive(Clone, Debug, Default)]
pub struct FocusRequests(Arc<Mutex<Option<FieldKey>>>);

impl FocusRequests {
  /// Ask for `key` to be focused after the views are next rebuilt. A later
  /// request replaces an earlier one not yet served.
  pub fn request(&self, key: FieldKey) {
    *self.0.lock().expect("focus requests poisoned") = Some(key);
  }

  /// Take the pending request, if any.
  pub fn take(&self) -> Option<FieldKey> {
    self.0.lock().expect("focus requests poisoned").take()
  }
}
