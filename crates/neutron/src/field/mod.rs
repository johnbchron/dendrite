//! A single-line text field: xilem's `text_input`, with the controls the
//! chrome needs and xilem 0.4 does not expose.
//!
//! - **Size and face.** The inner `TextArea` gets an explicit font size and the
//!   app's font stack, instead of Masonry's fixed 15 px system face.
//! - **Focus.** The field reports gaining and losing focus, and Escape gives
//!   focus up (the text area passes Escape through unhandled).
//! - **Its own frame.** Masonry's `TextInput` paints its focused border in
//!   hard-coded white, which disappears on a light palette. Here the inner
//!   input is transparent and borderless, and the wrapper paints the ground,
//!   the border and a focus ring from the theme.
//!
//! The widget is a thin wrapper, [`FieldWidget`](widget::FieldWidget), around
//! Masonry's `TextInput`; the view mirrors xilem's `TextInput` view and adds
//! the rest.

mod view;
mod widget;

use masonry::peniko::Color;

pub use self::view::field;
use crate::theme::Theme;

/// What the wrapper itself reports, alongside the text area's own
/// [`TextAction`](masonry::widgets::TextAction)s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldAction {
  /// The field gained or lost keyboard focus.
  Focus {
    /// Whether the field now has focus.
    focused:    bool,
    /// Whether a click in the field caused it (rather than the key map).
    by_pointer: bool,
  },
}

/// The frame's colours, all taken from the theme.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Colors {
  ground: Color,
  border: Color,
  focus:  Color,
}

impl Colors {
  fn from_theme(theme: &Theme) -> Self {
    Self {
      ground: theme.sunken,
      border: theme.rule,
      focus:  theme.focus,
    }
  }
}
