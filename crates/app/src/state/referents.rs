//! Editing the referent behind the selected formula condition, from its
//! card in the inspector: renaming a place, setting a balance, adding and
//! removing a schedule's windows (plans/formula-conditions.md, "Inspector").
//!
//! Every edit is an event, so it is undoable and synced like any other; the
//! atom points at the referent by id, so none of them changes which node
//! the condition is.

use base::{Atom, Event};

use super::{AppState, LiveEdit};
use crate::formula::{describe, phrase};

/// What has been typed into the referent card's fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Drafts {
  /// The referent's name.
  name:    String,
  /// A resource's balance.
  balance: String,
  /// A window to add to a schedule.
  span:    String,
}

impl AppState {
  /// The referent name field's text.
  pub fn referent_name_draft(&self) -> &str { &self.drafts.name }

  /// The balance field's text.
  pub fn balance_draft(&self) -> &str { &self.drafts.balance }

  /// The new-window field's text.
  pub fn span_draft(&self) -> &str { &self.drafts.span }

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
    let atom = self.selected_atom();
    let (key, event) = {
      let store = self.lock();
      let graph = store.graph();
      match atom {
        Some(Atom::At { place }) => {
          let Some(p) = graph.place(place).filter(|p| p.name != name) else {
            return;
          };
          (place.to_u128(), Event::PlaceChanged {
            place,
            name,
            within: p.within,
          })
        }
        Some(Atom::Has { resource, .. }) => {
          if graph.resource(resource).is_none_or(|r| r.name == name) {
            return;
          }
          (resource.to_u128(), Event::ResourceRenamed {
            resource,
            name,
          })
        }
        Some(Atom::Within { schedule }) => {
          let Some(s) = graph.schedule(schedule).filter(|s| s.name != name)
          else {
            return;
          };
          (schedule.to_u128(), Event::ScheduleChanged {
            schedule,
            name,
            spans: s.spans.clone(),
          })
        }
        Some(Atom::In { context }) => {
          if graph.context(context).is_none_or(|c| c.name == name) {
            return;
          }
          (context.to_u128(), Event::ContextRenamed { context, name })
        }
        _ => return,
      }
    };
    self.commit_live(LiveEdit::ReferentName(key), vec![event]);
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
    self.commit_live(LiveEdit::Balance(resource.to_u128()), vec![
      Event::ResourceBalanceSet { resource, balance },
    ]);
  }

  /// Enter in the balance field: close the edit, and tidy the text to the
  /// balance held.
  pub fn finish_balance(&mut self) {
    self.live_edit = None;
    self.reset_drafts();
  }

  /// The new-window field changed.
  pub fn set_span_draft(&mut self, text: String) { self.drafts.span = text; }

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
}
