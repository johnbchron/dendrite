//! The undo toast: a short-lived notice after an action worth undoing.

use super::AppState;

/// A short-lived notice with an Undo, shown after an action worth undoing
/// (deleting a node).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
  /// Distinguishes one toast from the next, so a timer for an old one does
  /// not take down a new one.
  pub id:   u64,
  /// What happened.
  pub text: String,
  /// The store revision right after the action; Undo is only offered while
  /// nothing has changed since.
  revision: u64,
}

/// The toast on screen, if any, and the counter that tells toasts apart.
#[derive(Default)]
pub(super) struct Toasts {
  current: Option<Toast>,
  /// Counter for [`Toast::id`].
  serial:  u64,
}

impl Toasts {
  /// Put up a toast saying `text`, offering Undo while the store stays at
  /// `revision`.
  pub(super) fn show(&mut self, text: String, revision: u64) {
    self.serial += 1;
    self.current = Some(Toast {
      id: self.serial,
      text,
      revision,
    });
  }

  /// The toast, if it is still current at the store's `revision`.
  fn live(&self, revision: u64) -> Option<&Toast> {
    self.current.as_ref().filter(|t| t.revision == revision)
  }

  /// Take the toast down.
  fn clear(&mut self) { self.current = None; }

  /// Take the toast down if it is the one numbered `id`.
  fn dismiss(&mut self, id: u64) {
    if self.current.as_ref().is_some_and(|t| t.id == id) {
      self.current = None;
    }
  }
}

impl AppState {
  /// The toast to show, if any: only while undoing would still undo what
  /// it names (nothing has changed the graph since).
  pub fn toast(&self) -> Option<&Toast> {
    let revision = self.lock().revision();
    self.toasts.live(revision)
  }

  /// The toast's Undo button.
  pub fn undo_toast(&mut self) {
    if self.toast().is_some() {
      self.undo();
    }
    self.toasts.clear();
  }

  /// Take the toast down: its timer ran out, or it was closed. `id` guards
  /// against a stale timer taking down a newer toast.
  pub fn dismiss_toast(&mut self, id: u64) { self.toasts.dismiss(id); }
}
