//! Editing the referent behind the selected formula condition, from its
//! card in the inspector: renaming a place, setting a balance, adding and
//! removing a schedule's windows (plans/formula-conditions.md, "Inspector").
//!
//! Every edit is an event, so it is undoable and synced like any other; the
//! atom points at the referent by id, so none of them changes which node
//! the condition is.
//!
//! Referents outlive the atoms that introduced them, so pruning removes
//! those no atom uses any more, with the formula conditions nothing
//! requires (see [`base::prune`]).

use base::{Atom, Event, NodeId};

use super::{AppState, LiveEdit, RefKey};
use crate::formula::{describe, phrase};

/// What has been typed into the referent card's fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Drafts {
  /// The referent's name.
  name: String,
  /// A resource's balance.
  balance: String,
  /// A window to add to a schedule.
  span: String,
}

impl AppState {
  /// The referent name field's text.
  pub fn referent_name_draft(&self) -> &str {
    &self.drafts.name
  }

  /// The balance field's text.
  pub fn balance_draft(&self) -> &str {
    &self.drafts.balance
  }

  /// The new-window field's text.
  pub fn span_draft(&self) -> &str {
    &self.drafts.span
  }

  /// The atom of the selected node, if it is a formula condition.
  fn selected_atom(&self) -> Option<Atom> {
    let id = self.selected?;
    self.lock().graph().node(id)?.kind.atom().cloned()
  }

  /// Fill the drafts from the selected formula condition's referent.
  pub(super) fn reset_drafts(&mut self) {
    // Before taking the lock: `selected_atom` takes it too.
    let atom = self.selected_atom();
    let drafts = {
      let store = self.lock();
      let graph = store.graph();
      match atom {
        Some(Atom::At { place }) => Drafts {
          name: graph
            .place(place)
            .map(|p| p.name.clone())
            .unwrap_or_default(),
          ..Drafts::default()
        },
        Some(Atom::Has { resource, .. }) => graph
          .resource(resource)
          .map(|r| Drafts {
            name: r.name.clone(),
            balance: describe::plain_amount(r),
            ..Drafts::default()
          })
          .unwrap_or_default(),
        Some(Atom::Within { schedule }) => Drafts {
          name: graph
            .schedule(schedule)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
          ..Drafts::default()
        },
        Some(Atom::In { context }) => Drafts {
          name: graph
            .context(context)
            .map(|c| c.name.clone())
            .unwrap_or_default(),
          ..Drafts::default()
        },
        _ => Drafts::default(),
      }
    };
    self.drafts = drafts;
  }

  /// The referent name field changed: keep the text as typed and rename
  /// the referent to its trimmed form, as the node name field does.
  pub fn rename_referent_to(&mut self, text: String) {
    self.drafts.name = text;
    let name = self.drafts.name.trim().to_string();
    if name.is_empty() {
      return;
    }
    let Some(key) = self.selected_atom().as_ref().and_then(RefKey::of) else {
      return;
    };
    let Some(event) = key.rename(self.lock().graph(), name) else {
      return;
    };
    self.commit_live(LiveEdit::ReferentName(key.raw()), vec![event]);
  }

  /// The balance field changed: keep the text as typed, and set the
  /// balance whenever it reads as an amount in the resource's unit. Typing
  /// a figure is one undo step.
  pub fn set_balance_text(&mut self, text: String) {
    self.drafts.balance = text;
    let Some(Atom::Has { resource, .. }) = self.selected_atom() else {
      return;
    };
    let balance = {
      let store = self.lock();
      let Some(r) = store.graph().resource(resource) else {
        return;
      };
      match phrase::amount(&self.drafts.balance, &r.unit) {
        Some(b) if b != r.balance => b,
        _ => return,
      }
    };
    self.commit_live(
      LiveEdit::Balance(resource.to_u128()),
      vec![Event::ResourceBalanceSet { resource, balance }],
    );
  }

  /// Enter in the balance field: close the edit, and tidy the text to the
  /// balance held.
  pub fn finish_balance(&mut self) {
    self.live_edit = None;
    self.reset_drafts();
  }

  /// The new-window field changed.
  pub fn set_span_draft(&mut self, text: String) {
    self.drafts.span = text;
  }

  /// Whether the new-window field reads as a window.
  pub fn span_draft_valid(&self) -> bool {
    phrase::span(&self.drafts.span, self.today()).is_some()
  }

  /// Add the window typed in the new-window field to the selected
  /// schedule, and clear the field. Nothing happens if it does not read as
  /// a window.
  pub fn add_span(&mut self) {
    let Some(Atom::Within { schedule }) = self.selected_atom() else {
      return;
    };
    let Some(span) = phrase::span(&self.drafts.span, self.today()) else {
      return;
    };
    let event = {
      let store = self.lock();
      let Some(s) = store.graph().schedule(schedule) else {
        return;
      };
      let mut spans = s.spans.clone();
      spans.push(span);
      Event::ScheduleChanged {
        schedule,
        name: s.name.clone(),
        spans,
      }
    };
    self.commit(vec![event]);
    self.drafts.span.clear();
  }

  /// Remove the selected schedule's `index`th window.
  pub fn remove_span(&mut self, index: usize) {
    let Some(Atom::Within { schedule }) = self.selected_atom() else {
      return;
    };
    let event = {
      let store = self.lock();
      let Some(s) = store.graph().schedule(schedule) else {
        return;
      };
      if index >= s.spans.len() {
        return;
      }
      let mut spans = s.spans.clone();
      spans.remove(index);
      Event::ScheduleChanged {
        schedule,
        name: s.name.clone(),
        spans,
      }
    };
    self.commit(vec![event]);
  }

  /// What [`prune`](Self::prune) would remove, in words ("2 places, 1
  /// context"), or `None` when there is nothing to prune.
  pub fn prune_summary(&self) -> Option<String> {
    let events = base::prune(self.lock().graph(), &self.facts());
    summarise(&events)
  }

  /// Remove every formula condition nothing requires and every referent
  /// no atom uses, as one undo step.
  pub fn prune(&mut self) {
    let events = base::prune(self.lock().graph(), &self.facts());
    let Some(summary) = summarise(&events) else {
      return;
    };
    let removed: Vec<NodeId> = events
      .iter()
      .filter_map(|e| match e {
        Event::NodeRemoved { node } => Some(*node),
        _ => None,
      })
      .collect();
    self.commit_as(events, "prune");
    if self.selected.is_some_and(|s| removed.contains(&s)) {
      self.select(None);
    }
    let revision = self.lock().revision();
    self.toasts.show(format!("Pruned {summary}"), revision);
  }
}

/// Count `events`' removals by kind, as "1 condition, 2 places".
fn summarise(events: &[Event]) -> Option<String> {
  let mut counts = [0usize; 5];
  for event in events {
    let slot = match event {
      Event::NodeRemoved { .. } => 0,
      Event::PlaceRemoved { .. } => 1,
      Event::ResourceRemoved { .. } => 2,
      Event::ScheduleRemoved { .. } => 3,
      Event::ContextRemoved { .. } => 4,
      _ => continue,
    };
    counts[slot] += 1;
  }
  let nouns = [
    ("condition", "conditions"),
    ("place", "places"),
    ("resource", "resources"),
    ("schedule", "schedules"),
    ("context", "contexts"),
  ];
  let parts: Vec<String> = counts
    .iter()
    .zip(nouns)
    .filter(|(n, _)| **n > 0)
    .map(|(&n, (one, many))| format!("{n} {}", if n == 1 { one } else { many }))
    .collect();
  (!parts.is_empty()).then(|| parts.join(", "))
}
