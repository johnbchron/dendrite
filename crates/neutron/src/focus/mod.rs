//! Where keyboard focus can be sent by name.
//!
//! A [`FieldKey`] names a text field the app can focus without knowing what
//! widget it is. Two halves hang off that name, and they sit on opposite
//! sides of every boundary in this app:
//!
//! - [`requests`] — the queue the app files a request in and the driver serves
//!   after the next rebuild. Plain data; no toolkit.
//! - [`registry`] — which Masonry widget currently *is* each field. Pure
//!   toolkit glue.
//!
//! They are apart because Masonry only lets a widget move focus while it is
//! handling an event, and the key map is the widget handling the keystroke
//! that asks for a field ("Enter to rename"). So fields register their text
//! area here, and the key map looks one up when the key arrives — while
//! anything that merely *wants* focus moved files a request instead.

mod registry;
mod requests;

pub use self::{
  registry::{lookup, register, unregister},
  requests::FocusRequests,
};

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
}
