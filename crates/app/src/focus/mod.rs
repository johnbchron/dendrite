//! Where keyboard focus can be sent by name.
//!
//! A [`FieldKey`] names a text field the app can focus without knowing what
//! widget it is, and [`FocusRequests`] is the queue it files a request in.
//! What actually moves focus lives with the UI, which is the only thing
//! that knows what a widget is: it keeps its own map from these names to
//! its fields and serves the queue after each rebuild.
//!
//! The indirection exists because a toolkit will typically only move focus
//! while handling an event, so the app cannot simply reach out and do it
//! when a command says "rename this".

mod requests;

pub use self::requests::FocusRequests;

/// A text field that can be focused by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldKey {
  /// The inspector's node name.
  Title,
  /// The inspector's requirement search, which arms link mode.
  LinkSearch,
  /// The command palette's search.
  PaletteSearch,
  /// The active quest's name, in the quest switcher.
  QuestName,
  /// The quest switcher's search.
  QuestSearch,
  /// The name of the referent behind the selected formula condition.
  ReferentName,
  /// The selected resource's balance.
  Balance,
  /// The field that adds a window to the selected schedule.
  Span,
  /// The Now tray's free time.
  FreeUntil,
  /// The selected condition's "Satisfied by" form.
  Source,
}
