//! The `meta` table: string settings keyed by name.

use rusqlite::{Connection, OptionalExtension as _};

use crate::DbError;

/// Reads and writes the `meta` table over a borrowed connection (or
/// transaction).
pub(crate) struct Meta<'c>(pub(crate) &'c Connection);

impl Meta<'_> {
  /// The value stored under `key`, or `None` if it was never written.
  pub(crate) fn get(&self, key: &str) -> Result<Option<String>, DbError> {
    let value = self
      .0
      .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| {
        r.get::<_, String>(0)
      })
      .optional()?;
    Ok(value)
  }

  /// Store `value` under `key`, replacing any previous value.
  pub(crate) fn set(&self, key: &str, value: &str) -> Result<(), DbError> {
    self.0.execute(
      "INSERT INTO meta (key, value) VALUES (?1, ?2)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      [key, value],
    )?;
    Ok(())
  }
}
