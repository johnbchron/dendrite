//! The `events` table: the append-only log that is the store's source of
//! truth.

use base::{Event, Graph};

use crate::{DbError, Store};

/// A single event as stored in (and read back from) the log.
struct StoredEvent {
  /// The event's own ULID (as a string).
  id:      String,
  /// The serialized [`base::Event`] payload.
  payload: String,
}

impl StoredEvent {
  /// Serialize `event` under a freshly minted id.
  fn new(event: &Event) -> Result<Self, DbError> {
    Ok(Self {
      id:      base::EventId::new().to_string(),
      payload: serde_json::to_string(event)?,
    })
  }
}

impl Store {
  /// Fold every stored event, in `seq` order, into a fresh graph. Returns
  /// it and the `seq` of the last event in the log (0 for an empty log).
  pub(crate) fn replay(&self) -> Result<(Graph, i64), DbError> {
    let mut stmt = self
      .conn
      .prepare("SELECT seq, payload FROM events ORDER BY seq ASC")?;
    let rows = stmt.query_map([], |row| {
      Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut graph = Graph::new();
    let mut last = 0;
    for row in rows {
      let (seq, payload) = row?;
      let event: Event = serde_json::from_str(&payload)
        .map_err(|error| DbError::BadEvent { seq, error })?;
      event.apply(&mut graph);
      last = seq;
    }
    Ok((graph, last))
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

  /// Append `events` to the log, in one transaction.
  pub(crate) fn persist(&mut self, events: &[Event]) -> Result<(), DbError> {
    // Serialize + mint ids up front so a failure leaves the DB untouched.
    let stored: Vec<StoredEvent> = events
      .iter()
      .map(StoredEvent::new)
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
    tx.commit()?;
    self.last_seq = last_seq;
    Ok(())
  }
}
