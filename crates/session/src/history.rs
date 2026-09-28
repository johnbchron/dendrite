//! Committing groups of events, and walking them back and forth with undo
//! and redo.
//!
//! **Undo never deletes history.** Undoing a group appends the inverse
//! events to the log (keeping it monotonic — the log doubles as an audit
//! trail) while walking the in-memory graph backwards (PLAN §3). The one
//! rewrite is inside a group still being built: when [`Session::commit_amend`]
//! adds an event that [supersedes](base::Event::supersedes) the group's last
//! one — the next keystroke of a rename — it replaces that entry, so a typed
//! name is logged once rather than once per character.
//!
//! A committed group may grow: a formula condition the group leaves with no
//! dependents is removed as part of it (see [`base::apply_group`]), so the
//! log and the undo step both carry the removal.

use base::Event;

use crate::{Error, Session};

/// One undoable step: the events committed together, and a short verb
/// phrase naming it ("rename").
pub struct Group {
  /// Applied in order to take the step.
  pub events: Vec<Event>,
  /// What the step does, for "Undo rename".
  pub label:  &'static str,
}

impl Session {
  /// Whether there is a group available to [`undo`](Session::undo).
  pub fn can_undo(&self) -> bool { !self.undo.is_empty() }

  /// Whether there is a group available to [`redo`](Session::redo).
  pub fn can_redo(&self) -> bool { !self.redo.is_empty() }

  /// What [`undo`](Session::undo) would reverse, as a short verb phrase
  /// ("rename"), or `None` when there is nothing to undo.
  pub fn undo_label(&self) -> Option<&'static str> {
    self.undo.last().map(|g| g.label)
  }

  /// What [`redo`](Session::redo) would re-apply, as for
  /// [`undo_label`](Session::undo_label).
  pub fn redo_label(&self) -> Option<&'static str> {
    self.redo.last().map(|g| g.label)
  }

  /// Apply and persist a batch of events as one undoable group.
  ///
  /// The batch is folded into the graph (yielding its inverse, and the
  /// removal of any formula condition it orphans), then the events are
  /// appended to the log. On success the inverse is pushed to the undo
  /// stack and the redo stack is cleared.
  pub fn commit(&mut self, events: Vec<Event>) -> Result<(), Error> {
    let label = events.first().map(Event::describe).unwrap_or_default();
    self.commit_labelled(events, label)
  }

  /// [`commit`](Session::commit) a batch whose undo step is named `label`
  /// rather than after its first event: for a gesture made of several
  /// kinds of event, such as turning a condition into another kind.
  pub fn commit_labelled(
    &mut self,
    events: Vec<Event>,
    label: &'static str,
  ) -> Result<(), Error> {
    if events.is_empty() {
      return Ok(());
    }
    let (events, inverse) = base::apply_group(&mut self.graph, events);
    self.revision += 1;
    self.last_seq = self.backend.append(&events)?;
    self.undo.push(Group {
      events: inverse,
      label,
    });
    self.redo.clear();
    self.tail = events.last().map(|e| (self.last_seq, e.clone()));
    Ok(())
  }

  /// Apply and persist a batch as part of the most recent undo group rather
  /// than as a new one, so a single [`undo`](Session::undo) reverses both.
  ///
  /// This is how a live edit (a rename committed on every keystroke) stays
  /// one undo step. With nothing to amend it is a plain
  /// [`commit`](Session::commit).
  pub fn commit_amend(&mut self, events: Vec<Event>) -> Result<(), Error> {
    if events.is_empty() {
      return Ok(());
    }
    if self.can_undo()
      && let [event] = events.as_slice()
      && let Some((at, last)) = &self.tail
      && event.supersedes(last)
    {
      return self.replace_tail(*at, event.clone());
    }
    let Some(prev) = self.undo.pop() else {
      return self.commit(events);
    };
    let (events, mut inverse) = base::apply_group(&mut self.graph, events);
    self.revision += 1;
    match self.backend.append(&events) {
      Ok(seq) => self.last_seq = seq,
      Err(e) => {
        self.undo.push(prev);
        return Err(e);
      }
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

  /// Apply `event`, which supersedes the logged event at `at`, by
  /// overwriting that entry instead of appending.
  ///
  /// The group's inverse needs no change: it already restores the field to
  /// its value from before the group, and `event` only moves the same
  /// field again.
  fn replace_tail(&mut self, at: i64, event: Event) -> Result<(), Error> {
    self.backend.replace(at, &event)?;
    event.apply(&mut self.graph);
    self.revision += 1;
    self.redo.clear();
    self.tail = Some((at, event));
    Ok(())
  }

  /// Reverse the most recently committed (or redone) group.
  ///
  /// The inverse batch is applied to the graph and *also appended* to the
  /// log, so history stays monotonic — nothing is ever deleted.
  pub fn undo(&mut self) -> Result<(), Error> {
    let Some(group) = self.undo.pop() else {
      return Ok(());
    };
    self.tail = None;
    let redo = base::apply_batch(&mut self.graph, &group.events);
    self.revision += 1;
    self.last_seq = self.backend.append(&group.events)?;
    self.redo.push(Group {
      events: redo,
      label:  group.label,
    });
    Ok(())
  }

  /// Re-apply the most recently undone group.
  pub fn redo(&mut self) -> Result<(), Error> {
    let Some(group) = self.redo.pop() else {
      return Ok(());
    };
    self.tail = None;
    let inverse = base::apply_batch(&mut self.graph, &group.events);
    self.revision += 1;
    self.last_seq = self.backend.append(&group.events)?;
    self.undo.push(Group {
      events: inverse,
      label:  group.label,
    });
    Ok(())
  }
}
