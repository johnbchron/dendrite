//! The key map: global keyboard shortcuts.
//!
//! Masonry sends key events to the focused widget and bubbles them up
//! through its ancestors; when nothing has focus they go to the window's
//! root widget (the "focus fallback"). [`KeymapWidget`](widget::KeymapWidget)
//! wraps the whole view tree, so it is that root, and it also sees any key a
//! focused text field leaves unhandled. It resolves keys with the pure
//! [`Binding::for_key`] and either emits a [`Command`] for the app to run or
//! moves focus to a named field.
//!
//! Single-letter shortcuts only fire while no text field has focus, because
//! a focused field consumes printable keys before they get here.

mod bind;
#[cfg(test)]
mod tests;
mod view;
mod widget;

pub use self::{
  bind::{Binding, Flags, chord},
  view::keymap,
};
use crate::canvas::ZoomStep;

/// Something the app does in response to a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
  /// Undo the last change.
  Undo,
  /// Redo the last undone change.
  Redo,
  /// Delete the selected node.
  Delete,
  /// Back out of whatever is open: link mode, then a popover, then the
  /// selection.
  Escape,
  /// Move the open query's highlight up (negative) or down.
  Move(isize),
  /// Act on the open query's highlighted result.
  Accept,
  /// Create a node (a condition with `condition`), attached to the
  /// selection if there is one.
  New {
    /// A condition rather than a task.
    condition: bool,
  },
  /// The selection's primary action (complete, reopen, satisfy...).
  Primary,
  /// Move the selection to a neighbour on the canvas.
  Nav(Direction),
  /// Step the canvas zoom.
  Zoom(ZoomStep),
  /// Fit the graph into view.
  Fit,
  /// Open or close the quest switcher.
  Quests,
  /// Open or close the Now tray.
  Now,
  /// Open (or close) the command palette; `nodes_only` to search nodes.
  Palette {
    /// Search nodes only (opened with `/`).
    nodes_only: bool,
  },
}

/// A direction on the canvas, for moving the selection with the arrows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
  /// To a node that requires this one (the row above).
  Up,
  /// To a node this one requires (the row below).
  Down,
  /// To the previous node in the same row.
  Left,
  /// To the next node in the same row.
  Right,
}
