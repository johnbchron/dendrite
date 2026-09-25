//! UI preferences, kept in `meta` beside the graph rather than in it.

use crate::{DbError, Store, meta::Meta};

impl Store {
  /// Read a UI preference from the `meta` table, or `None` if it was never
  /// written.
  ///
  /// Preferences live outside the event log on purpose: they are not graph
  /// data, so changing one must not appear on the undo stack or in the audit
  /// trail.
  pub fn setting(&self, key: &str) -> Result<Option<String>, DbError> {
    Meta(&self.conn).get(key)
  }

  /// Write a UI preference, replacing any previous value.
  pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
    Meta(&self.conn).set(key, value)
  }
}
