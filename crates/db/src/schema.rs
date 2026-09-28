//! Creating the tables, and refusing a database from a newer build.

use rusqlite::Connection;

use crate::{DbError, meta::Meta};

/// The current schema version, bumped when the table layout changes or the
/// log gains events an older build cannot read.
///
/// Version 2 dropped the `nodes`/`edges`/`quests`/`quest_claims` projection
/// tables: the log is the only source of truth, and it is replayed in full
/// on open.
///
/// Version 3 changes no table, only what the log may hold: formula
/// conditions and the referents they point at (plans/formula-conditions.md).
/// A version 2 build cannot parse those events, so the bump makes it refuse
/// the database cleanly rather than fail partway through replay.
const SCHEMA_VERSION: i64 = 3;

/// The database's table layout.
pub(crate) struct Schema<'c>(pub(crate) &'c Connection);

impl Schema<'_> {
  /// Refuse a database from a newer build, then create the tables if they
  /// do not yet exist.
  pub(crate) fn migrate(&self) -> Result<(), DbError> {
    if let Some(found) = self.version()?
      && found > SCHEMA_VERSION
    {
      return Err(DbError::NewerSchema {
        found,
        supported: SCHEMA_VERSION,
      });
    }
    self.0.execute_batch(
      "
      CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS events (
        seq     INTEGER PRIMARY KEY AUTOINCREMENT,
        id      TEXT NOT NULL,
        payload TEXT NOT NULL
      );

      -- Shed a v1 database's projection tables and the position they
      -- claimed to hold; the log alone rebuilds the graph.
      DROP TABLE IF EXISTS quest_claims;
      DROP TABLE IF EXISTS quests;
      DROP TABLE IF EXISTS edges;
      DROP TABLE IF EXISTS nodes;
      DELETE FROM meta WHERE key = 'snapshot_seq';
      ",
    )?;
    Meta(self.0).set("schema_version", &SCHEMA_VERSION.to_string())
  }

  /// The schema version recorded in `meta`, or `None` for a database that
  /// has none yet (brand new, or never migrated).
  fn version(&self) -> Result<Option<i64>, DbError> {
    let has_meta: bool = self.0.query_row(
      "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND \
       name = 'meta')",
      [],
      |r| r.get(0),
    )?;
    if !has_meta {
      return Ok(None);
    }
    let value = Meta(self.0).get("schema_version")?;
    // An unparseable version is treated as unknown-and-newer: better to
    // refuse than to write into a layout we do not understand.
    Ok(value.map(|v| v.parse().unwrap_or(i64::MAX)))
  }
}
