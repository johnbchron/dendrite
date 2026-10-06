//! Error type for the storage layer.

/// Anything that can go wrong while reading or writing the store.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
  /// A SQLite-level failure (I/O, constraint, migration, ...).
  #[error("sqlite error: {0}")]
  Sqlite(#[from] rusqlite::Error),
  /// An event payload failed to (de)serialize as JSON.
  #[error("event json error: {0}")]
  Json(#[from] serde_json::Error),
  /// The database was written by a newer build, whose schema or events this
  /// build may not understand. Refused before anything is read or written,
  /// so the file is left exactly as the newer build left it.
  #[error(
    "database schema version {found} is newer than this build supports \
     ({supported}); open it with a newer build of Dendrite"
  )]
  NewerSchema {
    /// Schema version recorded in the database.
    found: i64,
    /// Newest schema version this build can open.
    supported: i64,
  },
  /// The event at log position `seq` could not be read back. Opening stops
  /// there rather than skipping it: every later event may depend on it.
  #[error(
    "event #{seq} in the log could not be read ({error}); the database has \
     not been modified"
  )]
  BadEvent {
    /// The event's `seq` in the `events` table.
    seq: i64,
    /// Why its payload did not parse.
    #[source]
    error: serde_json::Error,
  },
}
