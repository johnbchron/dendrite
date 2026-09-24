//! Error type for the storage layer.

use std::fmt;

/// Anything that can go wrong while reading or writing the store.
#[derive(Debug)]
pub enum DbError {
  /// A SQLite-level failure (I/O, constraint, migration, ...).
  Sqlite(rusqlite::Error),
  /// An event payload failed to (de)serialize as JSON.
  Json(serde_json::Error),
  /// The database was written by a newer build, whose schema or events this
  /// build may not understand. Refused before anything is read or written,
  /// so the file is left exactly as the newer build left it.
  NewerSchema {
    /// Schema version recorded in the database.
    found:     i64,
    /// Newest schema version this build can open.
    supported: i64,
  },
  /// The event at log position `seq` could not be read back. Opening stops
  /// there rather than skipping it: every later event may depend on it.
  BadEvent {
    /// The event's `seq` in the `events` table.
    seq:   i64,
    /// Why its payload did not parse.
    error: serde_json::Error,
  },
}

impl fmt::Display for DbError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      DbError::Sqlite(e) => write!(f, "sqlite error: {e}"),
      DbError::Json(e) => write!(f, "event json error: {e}"),
      DbError::NewerSchema { found, supported } => write!(
        f,
        "database schema version {found} is newer than this build supports \
         ({supported}); open it with a newer build of Neutron"
      ),
      DbError::BadEvent { seq, error } => write!(
        f,
        "event #{seq} in the log could not be read ({error}); the database \
         has not been modified"
      ),
    }
  }
}

impl std::error::Error for DbError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      DbError::Sqlite(e) => Some(e),
      DbError::Json(e) => Some(e),
      DbError::NewerSchema { .. } => None,
      DbError::BadEvent { error, .. } => Some(error),
    }
  }
}

impl From<rusqlite::Error> for DbError {
  fn from(e: rusqlite::Error) -> Self { DbError::Sqlite(e) }
}

impl From<serde_json::Error> for DbError {
  fn from(e: serde_json::Error) -> Self { DbError::Json(e) }
}
