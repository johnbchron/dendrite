//! `db` — the SQLite [`Backend`] for an editing [`Session`] (PLAN §3,
//! Milestone 2).
//!
//! One database file holds the whole global graph, event-sourced:
//!
//! - The `events` table is an **append-only log** and the sole source of truth.
//!   Every UI mutation is one [`base::Event`], stored as a self-describing JSON
//!   payload alongside a monotonic `seq` and a ULID.
//! - The `meta` table holds UI preferences, which are not graph data and so
//!   never reach the log or the undo stack.
//!
//! Opening replays the log in `seq` order into the in-memory
//! [`Graph`](base::Graph), the only projection there is. Grouping events into
//! undoable steps, and walking them with undo and redo, is the `session`
//! crate's job; this crate only reads and writes rows.

mod error;
mod log;
mod meta;
mod schema;

use std::path::Path;

use base::Event;
pub use error::DbError;
use rusqlite::Connection;
use session::{Backend, Session};

use self::{meta::Meta, schema::Schema};

/// An editing session backed by a SQLite file.
pub type Store = Session;

/// Open (creating if absent) a session backed by the file at `path`.
pub fn open(path: &Path) -> Result<Store, session::Error> {
  Session::new(Box::new(Sqlite::open(path)?))
}

/// Open a throwaway in-memory session, handy for tests.
pub fn open_in_memory() -> Result<Store, session::Error> {
  Session::new(Box::new(Sqlite::open_in_memory()?))
}

/// A SQLite connection, migrated and ready to serve as a session's log and
/// preference store.
pub struct Sqlite {
  conn: Connection,
}

impl Sqlite {
  /// Open (creating if absent) the database at `path`.
  pub fn open(path: &Path) -> Result<Self, DbError> {
    Self::from_connection(Connection::open(path)?)
  }

  /// Open a throwaway in-memory database.
  pub fn open_in_memory() -> Result<Self, DbError> {
    Self::from_connection(Connection::open_in_memory()?)
  }

  /// Shared construction path: configure the connection and migrate.
  fn from_connection(conn: Connection) -> Result<Self, DbError> {
    // WAL mode gives better read/write concurrency and durability; it is a
    // no-op for in-memory databases.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    Schema(&conn).migrate()?;
    Ok(Sqlite { conn })
  }
}

impl Backend for Sqlite {
  fn replay(&self) -> Result<(Vec<Event>, i64), session::Error> {
    Ok(self.read_log()?)
  }

  fn event_count(&self) -> Result<u64, session::Error> {
    let n: i64 =
      self
        .conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
    Ok(n as u64)
  }

  fn append(&mut self, events: &[Event]) -> Result<i64, session::Error> {
    Ok(self.append_events(events)?)
  }

  fn replace(&mut self, at: i64, event: &Event) -> Result<(), session::Error> {
    Ok(self.replace_event(at, event)?)
  }

  fn setting(&self, key: &str) -> Result<Option<String>, session::Error> {
    Ok(Meta(&self.conn).get(key)?)
  }

  fn set_setting(&self, key: &str, value: &str) -> Result<(), session::Error> {
    Ok(Meta(&self.conn).set(key, value)?)
  }
}
