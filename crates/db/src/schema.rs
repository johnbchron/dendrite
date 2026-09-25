//! Creating the tables, and refusing a database from a newer build.

use rusqlite::Connection;

use crate::{DbError, meta::Meta};

/// The current schema version, bumped when the table layout changes.
const SCHEMA_VERSION: i64 = 1;

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

      CREATE TABLE IF NOT EXISTS nodes (
        id         TEXT PRIMARY KEY,
        name       TEXT NOT NULL,
        kind       TEXT NOT NULL,
        order_hint REAL NOT NULL
      );

      CREATE TABLE IF NOT EXISTS edges (
        id   TEXT PRIMARY KEY,
        kind TEXT NOT NULL,
        from_node TEXT NOT NULL,
        to_node   TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS quests (
        id   TEXT PRIMARY KEY,
        name TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS quest_claims (
        quest_id TEXT NOT NULL,
        node_id  TEXT NOT NULL,
        PRIMARY KEY (quest_id, node_id)
      );
      ",
    )?;
    self.0.execute(
      "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
      [SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
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
