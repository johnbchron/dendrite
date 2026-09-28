//! The key map: global keyboard shortcuts.
//!
//! This is the decision table only: [`Binding::for_key`] turns a key and
//! the app's [`Flags`] into either a [`Command`] to run or a field to move
//! focus to. Delivering keys to it, and acting on what it says, is the UI's
//! job.
//!
//! Single-letter shortcuts only fire while no text field has focus, because
//! a focused field consumes printable keys before they get here.

mod bind;
#[cfg(test)]
mod tests;

pub use self::bind::{Binding, Flags, chord};
use crate::camera::ZoomStep;

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
