//! Plain geometry the layout speaks in.

/// A 2-D position on the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
  /// Horizontal coordinate.
  pub x: f64,
  /// Vertical coordinate (increases downward).
  pub y: f64,
}

/// The width and height of a node's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
  /// Horizontal extent.
  pub w: f64,
  /// Vertical extent.
  pub h: f64,
}

/// A vertical stretch a long edge runs straight through: the full height of
/// one row it skips, at the gap its [`Slot::Bend`] reserved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Channel {
  /// Horizontal position.
  pub x:      f64,
  /// Top of the row.
  pub top:    f64,
  /// Bottom of the row.
  pub bottom: f64,
}
