//! `db` — SQLite-backed persistence for Neutron (PLAN §3, Milestone 2).
//!
//! One database holds the whole global graph. The design is event-sourced:
//!
//! - The `events` table is an **append-only log** and the sole source of truth.
//!   Every UI mutation is one [`base::Event`], stored as a self-describing JSON
//!   payload alongside a monotonic `seq` and a ULID.
//! - The `nodes`, `edges`, `quests` and `quest_claims` tables are a
//!   **snapshot** of the graph as of log position `snapshot_seq` (in `meta`).
//!   Opening loads the snapshot and replays only the events after it, so
//!   startup does not grow with the length of the log. The snapshot is
//!   rewritten, together with its `snapshot_seq`, in one transaction every
//!   [`SNAPSHOT_EVERY`] events and when the store closes. It is a cache: if it
//!   is missing or unreadable, opening falls back to a full replay.
//! - **Undo never deletes history.** Undoing a group appends the inverse events
//!   to the log (keeping it monotonic — the log doubles as an audit trail)
//!   while walking the in-memory graph backwards (PLAN §3). The one rewrite is
//!   inside a group still being built: when [`Store::commit_amend`] adds an
//!   event that [supersedes](base::Event::supersedes) the group's last one —
//!   the next keystroke of a rename — it replaces that row, so a typed name is
//!   logged once rather than once per character.
//!
//! On [`Store::open`] the snapshot is loaded and the rest of the log is
//! replayed in `seq` order on top of it.

mod error;

use std::path::Path;

use base::{Edge, Event, Graph, Node, Quest};
pub use error::DbError;
use rusqlite::{Connection, OptionalExtension as _};

/// The current schema version, bumped when the table layout changes.
const SCHEMA_VERSION: i64 = 1;

/// How many events may accumulate after the snapshot before a commit
/// rewrites it. Rewriting is a full table rewrite, so it is amortised
/// rather than done per commit; replaying this many events on open is cheap.
pub const SNAPSHOT_EVERY: i64 = 500;

/// `meta` key holding the log position the snapshot tables reflect.
const SNAPSHOT_KEY: &str = "snapshot_seq";

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
  conn:         Connection,
  graph:        Graph,
  /// Inverse batches, newest last. Popping one and applying it undoes the
  /// most recent group.
  undo:         Vec<Vec<Event>>,
  /// Batches that re-apply undone groups, newest last.
  redo:         Vec<Vec<Event>>,
  /// `seq` of the newest event in the log (0 for an empty log).
  last_seq:     i64,
  /// Bumped every time the graph changes, so callers can cache anything
  /// derived from it and know when to recompute.
  revision:     u64,
  /// `seq` the snapshot tables reflect.
  snapshot_seq: i64,
  /// The last event of the newest undo group and its `seq`, while that
  /// group can still be amended; cleared by undo and redo, whose events
  /// are history and must never be rewritten.
  tail:         Option<(i64, Event)>,
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
      last_seq: 0,
      revision: 0,
      snapshot_seq: 0,
      tail: None,
    };
    store.migrate()?;
    let (graph, from) = store.load_snapshot()?.unwrap_or_default();
    let (graph, last_seq) = store.replay(graph, from)?;
    store.graph = graph;
    store.last_seq = last_seq;
    store.snapshot_seq = from;
    // Catch the snapshot up now, so the next open replays nothing.
    if store.last_seq != store.snapshot_seq {
      let tx = store.conn.transaction()?;
      Self::write_snapshot(&tx, &store.graph, last_seq)?;
      tx.commit()?;
      store.snapshot_seq = last_seq;
    }
    Ok(store)
  }

  /// Refuse a database from a newer build, then create the tables if they
  /// do not yet exist.
  fn migrate(&self) -> Result<(), DbError> {
    if let Some(found) = self.schema_version()?
      && found > SCHEMA_VERSION
    {
      return Err(DbError::NewerSchema {
        found,
        supported: SCHEMA_VERSION,
      });
    }
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

  /// The schema version recorded in `meta`, or `None` for a database that
  /// has none yet (brand new, or never migrated).
  fn schema_version(&self) -> Result<Option<i64>, DbError> {
    let has_meta: bool = self.conn.query_row(
      "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND \
       name = 'meta')",
      [],
      |r| r.get(0),
    )?;
    if !has_meta {
      return Ok(None);
    }
    let value: Option<String> = self
      .conn
      .query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |r| r.get(0),
      )
      .optional()?;
    // An unparseable version is treated as unknown-and-newer: better to
    // refuse than to write into a layout we do not understand.
    Ok(value.map(|v| v.parse().unwrap_or(i64::MAX)))
  }

  /// Fold every stored event after `after`, in `seq` order, into `graph`.
  /// Returns the graph and the `seq` of the last event in the log.
  fn replay(
    &self,
    mut graph: Graph,
    after: i64,
  ) -> Result<(Graph, i64), DbError> {
    let mut stmt = self.conn.prepare(
      "SELECT seq, payload FROM events WHERE seq > ?1 ORDER BY seq ASC",
    )?;
    let rows = stmt.query_map([after], |row| {
      Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut last = after;
    for row in rows {
      let (seq, payload) = row?;
      let event: Event = serde_json::from_str(&payload)
        .map_err(|error| DbError::BadEvent { seq, error })?;
      event.apply(&mut graph);
      last = seq;
    }
    Ok((graph, last))
  }

  /// The snapshot graph and the log position it reflects, or `None` when
  /// there is no usable snapshot: none recorded, one claiming a position
  /// past the end of the log, or rows that do not parse. The caller then
  /// replays from the start, so a bad snapshot costs time, never data.
  fn load_snapshot(&self) -> Result<Option<(Graph, i64)>, DbError> {
    let Some(seq) = self
      .setting(SNAPSHOT_KEY)?
      .and_then(|v| v.parse::<i64>().ok())
    else {
      return Ok(None);
    };
    let max: i64 = self.conn.query_row(
      "SELECT COALESCE(MAX(seq), 0) FROM events",
      [],
      |r| r.get(0),
    )?;
    if seq > max {
      return Ok(None);
    }
    Ok(self.read_projections().map(|g| (g, seq)))
  }

  /// Rebuild a graph from the projection tables, or `None` if any row is
  /// unreadable.
  fn read_projections(&self) -> Option<Graph> {
    let mut graph = Graph::new();

    let mut stmt = self
      .conn
      .prepare("SELECT id, name, kind, order_hint FROM nodes")
      .ok()?;
    let rows = stmt
      .query_map([], |r| {
        Ok((
          r.get::<_, String>(0)?,
          r.get::<_, String>(1)?,
          r.get::<_, String>(2)?,
          r.get::<_, f64>(3)?,
        ))
      })
      .ok()?;
    for row in rows {
      let (id, name, kind, hint) = row.ok()?;
      let kind = serde_json::from_str(&kind).ok()?;
      graph.insert_node(Node::new(id.parse().ok()?, name, kind, hint));
    }

    let mut stmt = self
      .conn
      .prepare("SELECT id, kind, from_node, to_node FROM edges")
      .ok()?;
    let rows = stmt
      .query_map([], |r| {
        Ok((
          r.get::<_, String>(0)?,
          r.get::<_, String>(1)?,
          r.get::<_, String>(2)?,
          r.get::<_, String>(3)?,
        ))
      })
      .ok()?;
    for row in rows {
      let (id, kind, from, to) = row.ok()?;
      let kind = serde_json::from_str(&kind).ok()?;
      graph.insert_edge(Edge::new(
        id.parse().ok()?,
        kind,
        from.parse().ok()?,
        to.parse().ok()?,
      ));
    }

    let mut stmt = self.conn.prepare("SELECT id, name FROM quests").ok()?;
    let rows = stmt
      .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
      .ok()?;
    for row in rows {
      let (id, name) = row.ok()?;
      graph.insert_quest(Quest::new(id.parse().ok()?, name));
    }

    let mut stmt = self
      .conn
      .prepare("SELECT quest_id, node_id FROM quest_claims")
      .ok()?;
    let rows = stmt
      .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
      .ok()?;
    for row in rows {
      let (quest, node) = row.ok()?;
      graph.claim(quest.parse().ok()?, node.parse().ok()?);
    }

    Some(graph)
  }

  /// The current in-memory graph projection.
  pub fn graph(&self) -> &Graph { &self.graph }

  /// A counter that changes whenever [`Store::graph`] does (commit, amend,
  /// undo, redo) and never otherwise. Equal revisions mean an equal graph.
  pub fn revision(&self) -> u64 { self.revision }

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
    self.revision += 1;
    self.persist(&events)?;
    self.undo.push(inverse);
    self.redo.clear();
    self.tail = events.last().map(|e| (self.last_seq, e.clone()));
    Ok(())
  }

  /// Apply and persist a batch as part of the most recent undo group rather
  /// than as a new one, so a single [`undo`](Store::undo) reverses both.
  ///
  /// This is how a live edit (a rename committed on every keystroke) stays
  /// one undo step. With nothing to amend it is a plain
  /// [`commit`](Store::commit).
  pub fn commit_amend(&mut self, events: Vec<Event>) -> Result<(), DbError> {
    if events.is_empty() {
      return Ok(());
    }
    if self.can_undo()
      && let [event] = events.as_slice()
      && let Some((seq, last)) = &self.tail
      && event.supersedes(last)
    {
      return self.replace_tail(*seq, event.clone());
    }
    let Some(prev) = self.undo.pop() else {
      return self.commit(events);
    };
    let mut inverse = base::apply_batch(&mut self.graph, &events);
    self.revision += 1;
    if let Err(e) = self.persist(&events) {
      self.undo.push(prev);
      return Err(e);
    }
    // Undo runs the batch in order: first back out the amendment, then the
    // group it was folded into.
    inverse.extend(prev);
    self.undo.push(inverse);
    self.redo.clear();
    self.tail = events.last().map(|e| (self.last_seq, e.clone()));
    Ok(())
  }

  /// Apply `event`, which supersedes the logged event at `seq`, by
  /// overwriting that row instead of appending.
  ///
  /// The group's inverse needs no change: it already restores the field to
  /// its value from before the group, and `event` only moves the same
  /// field again.
  fn replace_tail(&mut self, seq: i64, event: Event) -> Result<(), DbError> {
    let payload = serde_json::to_string(&event)?;
    event.apply(&mut self.graph);
    self.revision += 1;
    let tx = self.conn.transaction()?;
    tx.execute(
      "UPDATE events SET payload = ?1 WHERE seq = ?2",
      (&payload, seq),
    )?;
    // A snapshot taken since the row was first written holds its old value.
    let stale = seq <= self.snapshot_seq;
    if stale {
      Self::write_snapshot(&tx, &self.graph, self.last_seq)?;
    }
    tx.commit()?;
    if stale {
      self.snapshot_seq = self.last_seq;
    }
    self.redo.clear();
    self.tail = Some((seq, event));
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
    self.tail = None;
    let redo = base::apply_batch(&mut self.graph, &inverse);
    self.revision += 1;
    self.persist(&inverse)?;
    self.redo.push(redo);
    Ok(())
  }

  /// Re-apply the most recently undone group.
  pub fn redo(&mut self) -> Result<(), DbError> {
    let Some(forward) = self.redo.pop() else {
      return Ok(());
    };
    self.tail = None;
    let inverse = base::apply_batch(&mut self.graph, &forward);
    self.revision += 1;
    self.persist(&forward)?;
    self.undo.push(inverse);
    Ok(())
  }

  /// Read a UI preference from the `meta` table, or `None` if it was never
  /// written.
  ///
  /// Preferences live outside the event log on purpose: they are not graph
  /// data, so changing one must not appear on the undo stack or in the audit
  /// trail.
  pub fn setting(&self, key: &str) -> Result<Option<String>, DbError> {
    let value = self
      .conn
      .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| {
        r.get::<_, String>(0)
      })
      .optional()?;
    Ok(value)
  }

  /// Write a UI preference, replacing any previous value.
  pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
    self.conn.execute(
      "INSERT INTO meta (key, value) VALUES (?1, ?2)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      [key, value],
    )?;
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

  /// Append `events` to the log, in one transaction. When the log has run
  /// [`SNAPSHOT_EVERY`] events past the snapshot, the snapshot is rewritten
  /// in the same transaction.
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
    let last_seq = tx.last_insert_rowid();
    let snapshot = last_seq - self.snapshot_seq >= SNAPSHOT_EVERY;
    if snapshot {
      Self::write_snapshot(&tx, &self.graph, last_seq)?;
    }
    tx.commit()?;
    self.last_seq = last_seq;
    if snapshot {
      self.snapshot_seq = last_seq;
    }
    Ok(())
  }

  /// Rewrite the snapshot tables from `graph` and record that they reflect
  /// the log up to `seq`.
  fn write_snapshot(
    conn: &Connection,
    graph: &Graph,
    seq: i64,
  ) -> Result<(), DbError> {
    Self::write_projections(conn, graph)?;
    conn.execute(
      "INSERT INTO meta (key, value) VALUES (?1, ?2)
       ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      (SNAPSHOT_KEY, seq.to_string()),
    )?;
    Ok(())
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

impl Drop for Store {
  /// Catch the snapshot up on close, so the next open replays nothing. Best
  /// effort: if it fails, the next open replays the gap from the log.
  fn drop(&mut self) {
    if self.last_seq == self.snapshot_seq {
      return;
    }
    let Ok(tx) = self.conn.transaction() else {
      return;
    };
    if Self::write_snapshot(&tx, &self.graph, self.last_seq).is_ok() {
      let _ = tx.commit();
    }
  }
}
