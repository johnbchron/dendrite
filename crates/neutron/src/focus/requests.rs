//! The queue of pending focus requests: plain data, shared between the app
//! state, which files a request, and [`crate::driver::FocusDriver`], which
//! serves it after the next rebuild.

use std::sync::{Arc, Mutex};

use super::FieldKey;

/// Fields the app has asked to focus once the views catch up.
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
