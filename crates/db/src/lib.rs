//! `db` — SQLite-backed persistence for Neutron (PLAN §3, Milestone 2).
//!
//! One database holds the whole global graph. The design is event-sourced:
//!
//! - The `events` table is an **append-only log** and the sole source of truth.
//!   Every UI mutation is one [`base::Event`], stored as a self-describing JSON
//!   payload alongside a monotonic `seq` and a ULID.
//! - The `nodes`, `edges`, `quests` and `quest_claims` tables are
//!   **projections** materialized from the log for fast reads. They are rebuilt
//!   from the in-memory [`base::Graph`] inside the same transaction that
//!   appends events, so a reader always sees a consistent snapshot.
//! - **Undo never deletes history.** Undoing a group appends the inverse events
//!   to the log (keeping it monotonic — the log doubles as an audit trail)
//!   while walking the in-memory graph backwards (PLAN §3).
//!
//! On [`Store::open`] the log is replayed in `seq` order to rebuild the
//! in-memory graph, and the projection tables are refreshed from it.

mod error;

use std::path::Path;

use base::{Event, Graph};
pub use error::DbError;
use rusqlite::Connection;

/// The current schema version, bumped when the table layout changes.
const SCHEMA_VERSION: i64 = 1;

/// A single event as stored in (and read back from) the log.
struct StoredEvent {
  /// The event's own ULID (as a string).
  id:      String,
  /// The serialized [`base::Event`] payload.
  payload: String,
}

/// The persistent store: a SQLite connection plus the in-memory graph
/// projection it drives, and the undo/redo stacks.
///
/// Each entry on a stack is one *group* — the set of events committed
/// together — so a single `undo`/`redo` reverses a whole user action.
pub struct Store {
  conn:  Connection,
  graph: Graph,
  /// Inverse batches, newest last. Popping one and applying it undoes the
  /// most recent group.
  undo:  Vec<Vec<Event>>,
  /// Batches that re-apply undone groups, newest last.
  redo:  Vec<Vec<Event>>,
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
  /// replay the log into the graph, and refresh projections.
  fn from_connection(conn: Connection) -> Result<Self, DbError> {
    // WAL mode gives better read/write concurrency and durability; it is a
    // no-op for in-memory databases.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    let mut store = Store {
      conn,
      graph: Graph::new(),
      undo: Vec::new(),
      redo: Vec::new(),
    };
    store.migrate()?;
    store.graph = store.replay()?;
    store.rebuild_projections()?;
    Ok(store)
  }

  /// Create the tables if they do not yet exist.
  fn migrate(&self) -> Result<(), DbError> {
    self.conn.execute_batch(
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
    self.conn.execute(
      "INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', ?1)",
      [SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
  }

  /// Fold every stored event, in `seq` order, into a fresh graph.
  fn replay(&self) -> Result<Graph, DbError> {
    let mut stmt = self
      .conn
      .prepare("SELECT payload FROM events ORDER BY seq ASC")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut graph = Graph::new();
    for row in rows {
      let payload = row?;
      let event: Event = serde_json::from_str(&payload)?;
      event.apply(&mut graph);
    }
    Ok(graph)
  }

  /// The current in-memory graph projection.
  pub fn graph(&self) -> &Graph { &self.graph }

  /// Whether there is a group available to [`undo`](Store::undo).
  pub fn can_undo(&self) -> bool { !self.undo.is_empty() }

  /// Whether there is a group available to [`redo`](Store::redo).
  pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

  /// Apply and persist a batch of events as one undoable group.
  ///
  /// The batch is folded into the in-memory graph (yielding its inverse),
  /// then the events are appended to the log and the projections rebuilt in
  /// a single transaction. On success the inverse is pushed to the undo
  /// stack and the redo stack is cleared.
  pub fn commit(&mut self, events: Vec<Event>) -> Result<(), DbError> {
    if events.is_empty() {
      return Ok(());
    }
    let inverse = base::apply_batch(&mut self.graph, &events);
    self.persist(&events)?;
    self.undo.push(inverse);
    self.redo.clear();
    Ok(())
  }

  /// Reverse the most recently committed (or redone) group.
  ///
  /// The inverse batch is applied to the graph and *also appended* to the
  /// log, so history stays monotonic — nothing is ever deleted.
  pub fn undo(&mut self) -> Result<(), DbError> {
    let Some(inverse) = self.undo.pop() else {
      return Ok(());
    };
    let redo = base::apply_batch(&mut self.graph, &inverse);
    self.persist(&inverse)?;
    self.redo.push(redo);
    Ok(())
  }

  /// Re-apply the most recently undone group.
  pub fn redo(&mut self) -> Result<(), DbError> {
    let Some(forward) = self.redo.pop() else {
      return Ok(());
    };
    let inverse = base::apply_batch(&mut self.graph, &forward);
    self.persist(&forward)?;
    self.undo.push(inverse);
    Ok(())
  }

  /// Total number of events in the log (its length grows monotonically,
  /// including across undo/redo).
  pub fn event_count(&self) -> Result<u64, DbError> {
    let n: i64 =
      self
        .conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))?;
    Ok(n as u64)
  }

  /// Append `events` to the log and rebuild the projection tables from the
  /// current in-memory graph, all in one transaction.
  fn persist(&mut self, events: &[Event]) -> Result<(), DbError> {
    // Serialize + mint ids up front so a failure leaves the DB untouched.
    let stored: Vec<StoredEvent> = events
      .iter()
      .map(|e| {
        Ok(StoredEvent {
          id:      base::EventId::new().to_string(),
          payload: serde_json::to_string(e)?,
        })
      })
      .collect::<Result<_, DbError>>()?;

    let tx = self.conn.transaction()?;
    {
      let mut insert =
        tx.prepare("INSERT INTO events (id, payload) VALUES (?1, ?2)")?;
      for ev in &stored {
        insert.execute((&ev.id, &ev.payload))?;
      }
    }
    Self::write_projections(&tx, &self.graph)?;
    tx.commit()?;
    Ok(())
  }

  /// Rebuild the projection tables from `graph` outside of a caller-supplied
  /// transaction (used once at open time).
  fn rebuild_projections(&self) -> Result<(), DbError> {
    Self::write_projections(&self.conn, &self.graph)
  }

  /// Overwrite every projection table to exactly mirror `graph`.
  ///
  /// A full rewrite is trivially correct and more than fast enough at v1
  /// scale; incremental projection maintenance can replace it later.
  fn write_projections(
    conn: &Connection,
    graph: &Graph,
  ) -> Result<(), DbError> {
    conn.execute_batch(
      "DELETE FROM nodes;
       DELETE FROM edges;
       DELETE FROM quests;
       DELETE FROM quest_claims;",
    )?;

    {
      let mut stmt = conn.prepare(
        "INSERT INTO nodes (id, name, kind, order_hint) VALUES (?1, ?2, ?3, \
         ?4)",
      )?;
      for node in graph.nodes() {
        let kind = serde_json::to_string(&node.kind)?;
        stmt.execute((
          node.id.to_string(),
          &node.name,
          kind,
          node.order_hint,
        ))?;
      }
    }
    {
      let mut stmt = conn.prepare(
        "INSERT INTO edges (id, kind, from_node, to_node) VALUES (?1, ?2, ?3, \
         ?4)",
      )?;
      for edge in graph.edges() {
        let kind = serde_json::to_string(&edge.kind)?;
        stmt.execute((
          edge.id.to_string(),
          kind,
          edge.from.to_string(),
          edge.to.to_string(),
        ))?;
      }
    }
    {
      let mut quest_stmt =
        conn.prepare("INSERT INTO quests (id, name) VALUES (?1, ?2)")?;
      let mut claim_stmt = conn.prepare(
        "INSERT INTO quest_claims (quest_id, node_id) VALUES (?1, ?2)",
      )?;
      for quest in graph.quests() {
        quest_stmt.execute((quest.id.to_string(), &quest.name))?;
        for node in &quest.claims {
          claim_stmt.execute((quest.id.to_string(), node.to_string()))?;
        }
      }
    }
    Ok(())
  }
}
