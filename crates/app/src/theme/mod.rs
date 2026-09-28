//! The colour palettes the UI can be set to (PLAN §5).
//!
//! Both painted surfaces — the Vello canvas and the Xilem chrome — read their
//! colours from one [`Theme`], so a palette is a single value the app carries
//! rather than two sets of constants that can drift apart. Every
//! theme fills the same slots, and the meanings are fixed across all of
//! them: `accent` is selection, green is satisfied, red is a cycle, grey is
//! waiting. The accent is always a different hue from the Ready border, so a
//! selected Ready node is told apart from its unselected neighbours by colour
//! and not just stroke width.
//!
//! The chosen theme is a *preference*, not graph data: it is stored in the
//! `meta` table rather than the event log, so switching palettes never lands
//! on the undo stack.

use base::NodeState;
use peniko::Color;

mod palettes;

/// One complete palette: canvas colours, per-state node colours, and the
/// side panel's chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
  /// Stable key used to persist the choice. Never shown to the user.
  pub id:   &'static str,
  /// Display name, shown in the palette picker.
  pub name: &'static str,

  /// Canvas ground.
  pub bg:     Color,
  /// Ordinary edges and their arrowheads.
  pub edge:   Color,
  /// Edges the cycle-cut reversed.
  pub cycle:  Color,
  /// Node labels on the canvas, and body text in the panel.
  pub text:   Color,
  /// Selection, the Ready state, and interactive hints.
  pub accent: Color,

  /// `(fill, border)` for a completed task or satisfied condition.
  pub done:    (Color, Color),
  /// `(fill, border)` for a Ready node.
  pub ready:   (Color, Color),
  /// `(fill, border)` for a Blocked node.
  pub blocked: (Color, Color),
  /// `(fill, border)` for a Pending condition.
  pub pending: (Color, Color),
  /// `(fill, border)` for a node caught in a cycle.
  pub cyclic:  (Color, Color),

  /// Fields, framed button groups and other insets on a surface.
  pub sunken: Color,
  /// Hairlines.
  pub rule:   Color,
  /// Secondary text.
  pub muted:  Color,

  /// Cards and bars that sit over the canvas: the top bar, the inspector,
  /// the Now tray.
  pub surface:        Color,
  /// Popovers and the command palette, one step above `surface`.
  pub surface_raised: Color,
  /// Translucent wash over the canvas behind a modal overlay.
  pub scrim:          Color,
  /// Keyboard-focus ring on focusable controls.
  pub focus:          Color,
  /// Drop shadow under floating surfaces (usually translucent).
  pub shadow:         Color,
  /// Text and icons drawn on an `accent` fill (the primary button).
  pub on_accent:      Color,

  /// Chip ground for Ready.
  pub chip_ready:   Color,
  /// Chip ground for completed/satisfied.
  pub chip_done:    Color,
  /// Chip ground for blocked/pending.
  pub chip_blocked: Color,
  /// Chip ground for cyclic.
  pub chip_cyclic:  Color,
}

impl Theme {
  /// Every palette the picker offers, in the order it shows them.
  pub const ALL: &[&Theme] = &[
    &palettes::SLATE,
    &palettes::GRAPHITE,
    &palettes::UMBER,
    &palettes::FROST,
    &palettes::EVERGARDEN,
    &palettes::MERIDIAN,
  ];
  /// The palette used when nothing has been chosen (or a stored choice no
  /// longer exists).
  pub const DEFAULT: &Theme = &palettes::SLATE;

  /// `(fill, border)` for a node in the given derived state.
  pub fn for_state(&self, state: NodeState) -> (Color, Color) {
    match state {
      NodeState::Completed | NodeState::Satisfied => self.done,
      NodeState::Ready => self.ready,
      NodeState::Blocked => self.blocked,
      NodeState::Pending => self.pending,
      NodeState::Cyclic => self.cyclic,
    }
  }

  /// Chip ground for a node in the given derived state.
  pub fn chip_for_state(&self, state: NodeState) -> Color {
    match state {
      NodeState::Completed | NodeState::Satisfied => self.chip_done,
      NodeState::Ready => self.chip_ready,
      NodeState::Cyclic => self.chip_cyclic,
      NodeState::Blocked | NodeState::Pending => self.chip_blocked,
    }
  }

  /// Look a palette up by its persisted [`Theme::id`].
  pub fn by_id(id: &str) -> Option<&'static Theme> {
    Self::ALL.iter().copied().find(|t| t.id == id)
  }

  /// Dim a fill for pulled-in (unclaimed) nodes in a quest scope. Alpha
  /// rather than a fixed colour, so every palette dims toward its own ground.
  pub fn dim(c: Color) -> Color { c.multiply_alpha(0.5) }
}

#[cfg(test)]
mod tests;
