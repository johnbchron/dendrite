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
//! The widget is a thin wrapper, `FieldWidget`, around
//! Masonry's `TextInput`; the view mirrors xilem's `TextInput` view and adds
//! the rest.

mod view;
mod widget;

use masonry::{core::WidgetId, parley::style::FontStack, peniko::Color};

pub use self::view::{Field, field};

/// What the wrapper itself reports, alongside the text area's own
/// [`TextAction`](masonry::widgets::TextAction)s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldAction {
  /// The field gained or lost keyboard focus.
  Focus {
    /// Whether the field now has focus.
    focused: bool,
    /// Whether a click in the field caused it (rather than the key map).
    by_pointer: bool,
  },
}

/// How a field is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
  /// The inset ground behind the text.
  pub ground: Color,
  /// The frame at rest.
  pub border: Color,
  /// The frame, and the ring, while the field has focus.
  pub focus: Color,
  /// The text, and the caret.
  pub text: Color,
  /// Placeholder text.
  pub muted: Color,
  /// Corner radius of the frame.
  pub radius: f64,
  /// The face the text is shaped in.
  pub font: FontStack<'static>,
}

/// Told when the field's text area is mounted and when it goes, so the app
/// can keep its own map from names to widgets (see `dendrite::focus`).
///
/// Masonry will typically only move focus while handling an event, so
/// something that merely wants focus moved has to look the widget up.
pub trait Mount: Send + Sync + 'static {
  /// The text area now exists, with this id.
  fn mounted(&self, id: WidgetId);
  /// The text area with this id is going away.
  fn unmounted(&self, id: WidgetId);
}
