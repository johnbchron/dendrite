//! The `events` table: the append-only log that is the session's source of
//! truth.

use base::Event;
use rusqlite::Connection;

use crate::{DbError, Sqlite};

/// Serialize `event` under a freshly minted id, ready to insert.
fn stored(event: &Event) -> Result<(String, String), DbError> {
  Ok((
    base::EventId::new().to_string(),
    serde_json::to_string(event)?,
  ))
}

/// Insert `rows` of `(id, payload)`, returning the `seq` of the last one.
fn insert(
  conn: &Connection,
  rows: &[(String, String)],
) -> Result<i64, DbError> {
  let mut stmt =
    conn.prepare("INSERT INTO events (id, payload) VALUES (?1, ?2)")?;
  for (id, payload) in rows {
    stmt.execute((id, payload))?;
  }
  Ok(conn.last_insert_rowid())
}

impl Sqlite {
  /// Every stored event in `seq` order, and the `seq` of the last one
  /// (0 for an empty log).
  pub(crate) fn read_log(&self) -> Result<(Vec<Event>, i64), DbError> {
    let mut stmt = self
      .conn
      .prepare("SELECT seq, payload FROM events ORDER BY seq ASC")?;
    let rows = stmt.query_map([], |row| {
      Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut events = Vec::new();
    let mut last = 0;
    for row in rows {
      let (seq, payload) = row?;
      events.push(
        serde_json::from_str(&payload)
          .map_err(|error| DbError::BadEvent { seq, error })?,
      );
      last = seq;
    }
    Ok((events, last))
  }

  /// Append `events` to the log in one transaction, returning the `seq` of
  /// the last one.
  pub(crate) fn append_events(
    &mut self,
    events: &[Event],
  ) -> Result<i64, DbError> {
    // Serialize + mint ids up front so a failure leaves the DB untouched.
    let rows: Vec<(String, String)> =
      events.iter().map(stored).collect::<Result<_, DbError>>()?;
    let tx = self.conn.transaction()?;
    let last_seq = insert(&tx, &rows)?;
    tx.commit()?;
    Ok(last_seq)
  }

  /// Overwrite the payload of the event at `seq`, which a later event
  /// supersedes.
  pub(crate) fn replace_event(
    &mut self,
    seq: i64,
    event: &Event,
  ) -> Result<(), DbError> {
    let payload = serde_json::to_string(event)?;
    self.conn.execute(
      "UPDATE events SET payload = ?1 WHERE seq = ?2",
      (&payload, seq),
    )?;
    Ok(())
  }
}
