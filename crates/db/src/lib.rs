//! `db` — SQLite-backed persistence for Neutron (PLAN §3, Milestone 2).
//!
//! One database holds the whole global graph. The design is event-sourced:
//!
//! - The `events` table is an **append-only log** and the sole source of truth.
//!   Every UI mutation is one [`base::Event`], stored as a self-describing JSON
//!   payload alongside a monotonic `seq` and a ULID.
//! - **Undo never deletes history.** Undoing a group appends the inverse events
//!   to the log (keeping it monotonic — the log doubles as an audit trail)
//!   while walking the in-memory graph backwards (PLAN §3). The one rewrite is
//!   inside a group still being built: when [`Store::commit_amend`] adds an
//!   event that [supersedes](base::Event::supersedes) the group's last one —
//!   the next keystroke of a rename — it replaces that row, so a typed name is
//!   logged once rather than once per character.
//!
//! On [`Store::open`] the log is replayed in `seq` order into the in-memory
//! [`Graph`], which is the only projection there is.

mod error;
mod history;
mod log;
mod meta;
mod schema;
mod settings;

use std::path::Path;

use base::{Event, Graph};
pub use error::DbError;
use rusqlite::Connection;

use self::{history::Group, schema::Schema};

/// The persistent store: a SQLite connection plus the in-memory graph
/// projection it drives, and the undo/redo stacks.
///
/// Each entry on a stack is one *group* — the set of events committed
/// together — so a single `undo`/`redo` reverses a whole user action.
pub struct Store {
  conn:     Connection,
  graph:    Graph,
  /// Inverse batches, newest last, each with the label of the group it
  /// undoes. Popping one and applying it undoes the most recent group.
  undo:     Vec<Group>,
  /// Batches that re-apply undone groups, newest last, with their labels.
  redo:     Vec<Group>,
  /// `seq` of the newest event in the log (0 for an empty log).
  last_seq: i64,
  /// Bumped every time the graph changes, so callers can cache anything
  /// derived from it and know when to recompute.
  revision: u64,
  /// The last event of the newest undo group and its `seq`, while that
  /// group can still be amended; cleared by undo and redo, whose events
  /// are history and must never be rewritten.
  tail:     Option<(i64, Event)>,
}

impl Store {
  /// Open (creating if absent) a store backed by the file at `path`.
  pub fn open(path: &Path) -> Result<Self, DbError> {
    let conn = Connection::open(path)?;
    Self::from_connection(conn)
  }

  /// Open a throwaway in-memory store, handy for tests.
  pub fn open_in_memory() -> Result<Self, DbError> {
    let conn = Connection::open_in_memory()?;
    Self::from_connection(conn)
  }

  /// Shared construction path: configure the connection, run migrations,
  /// and replay the log into the graph.
  fn from_connection(conn: Connection) -> Result<Self, DbError> {
    // WAL mode gives better read/write concurrency and durability; it is a
    // no-op for in-memory databases.
    conn.pragma_update(None, "journal_mode", "WAL")?;

    let mut store = Store {
      conn,
      graph: Graph::new(),
      undo: Vec::new(),
      redo: Vec::new(),
      last_seq: 0,
      revision: 0,
      tail: None,
    };
    Schema(&store.conn).migrate()?;
    (store.graph, store.last_seq) = store.replay()?;
    Ok(store)
  }

  /// The current in-memory graph projection.
  pub fn graph(&self) -> &Graph { &self.graph }

  /// A counter that changes whenever [`Store::graph`] does (commit, amend,
  /// undo, redo) and never otherwise. Equal revisions mean an equal graph.
  pub fn revision(&self) -> u64 { self.revision }
}
