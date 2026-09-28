//! Delivering keys to the app's key map.
//!
//! Masonry sends key events to the focused widget and bubbles them up
//! through its ancestors; when nothing has focus they go to the window's
//! root widget (the "focus fallback"). [`KeymapWidget`](widget::KeymapWidget)
//! wraps the whole view tree, so it is that root, and it also sees any key a
//! focused text field leaves unhandled. It resolves each key with the pure
//! [`Binding::for_key`](app::keymap::Binding::for_key) and either emits a
//! [`Command`] for the app to run or moves focus to a named field.

mod view;
mod widget;

#[allow(unused_imports)]
pub use app::keymap::{Binding, Command, Direction, Flags, chord};

pub use self::view::keymap;
