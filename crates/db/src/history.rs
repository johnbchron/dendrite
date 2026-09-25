//! Committing groups of events, and walking them back and forth with undo
//! and redo.

use base::Event;

use crate::{DbError, Store, snapshot::Snapshot};

/// One undoable step: the events committed together, and a short verb
/// phrase naming it ("rename").
pub(crate) struct Group {
  /// Applied in order to take the step.
  pub(crate) events: Vec<Event>,
  /// What the step does, for "Undo rename".
  pub(crate) label:  &'static str,
}

impl Store {
  /// Whether there is a group available to [`undo`](Store::undo).
  pub fn can_undo(&self) -> bool { !self.undo.is_empty() }

  /// Whether there is a group available to [`redo`](Store::redo).
  pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

  /// What [`undo`](Store::undo) would reverse, as a short verb phrase
  /// ("rename"), or `None` when there is nothing to undo.
  pub fn undo_label(&self) -> Option<&'static str> {
    self.undo.last().map(|g| g.label)
  }

  /// What [`redo`](Store::redo) would re-apply, as for
  /// [`undo_label`](Store::undo_label).
  pub fn redo_label(&self) -> Option<&'static str> {
    self.redo.last().map(|g| g.label)
  }

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
    self.undo.push(Group {
      events: inverse,
      label:  events[0].describe(),
    });
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
    // group it was folded into. The group keeps the name it started with.
    inverse.extend(prev.events);
    self.undo.push(Group {
      events: inverse,
      label:  prev.label,
    });
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
      Snapshot(&tx).save(&self.graph, self.last_seq)?;
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
    let Some(group) = self.undo.pop() else {
      return Ok(());
    };
    self.tail = None;
    let redo = base::apply_batch(&mut self.graph, &group.events);
    self.revision += 1;
    self.persist(&group.events)?;
    self.redo.push(Group {
      events: redo,
      label:  group.label,
    });
    Ok(())
  }

  /// Re-apply the most recently undone group.
  pub fn redo(&mut self) -> Result<(), DbError> {
    let Some(group) = self.redo.pop() else {
      return Ok(());
    };
    self.tail = None;
    let inverse = base::apply_batch(&mut self.graph, &group.events);
    self.revision += 1;
    self.persist(&group.events)?;
    self.undo.push(Group {
      events: inverse,
      label:  group.label,
    });
    Ok(())
  }
}
