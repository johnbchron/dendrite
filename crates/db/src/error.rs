//! Error type for the storage layer.

use std::fmt;

/// Anything that can go wrong while reading or writing the store.
#[derive(Debug)]
pub enum DbError {
  /// A SQLite-level failure (I/O, constraint, migration, ...).
  Sqlite(rusqlite::Error),
  /// An event payload failed to (de)serialize as JSON.
  Json(serde_json::Error),
}

impl fmt::Display for DbError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      DbError::Sqlite(e) => write!(f, "sqlite error: {e}"),
      DbError::Json(e) => write!(f, "event json error: {e}"),
    }
  }
}

impl std::error::Error for DbError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      DbError::Sqlite(e) => Some(e),
      DbError::Json(e) => Some(e),
    }
  }
}

impl From<rusqlite::Error> for DbError {
  fn from(e: rusqlite::Error) -> Self { DbError::Sqlite(e) }
}

impl From<serde_json::Error> for DbError {
  fn from(e: serde_json::Error) -> Self { DbError::Json(e) }
}
