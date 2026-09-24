//! Design tokens: the one set of sizes every piece of chrome is built from.
//!
//! Colours live in [`crate::theme`], because they change with the palette.
//! Everything here is palette-independent, so it is plain constants. Code
//! outside this module should not write a literal padding, gap, radius or
//! type size; if a value is missing, it belongs here first.

/// Spacing, in logical pixels. Everything sits on a 4 px grid.
pub mod space {
  /// The one off-grid value: the inset inside a framed button group, and
  /// the gap between list rows.
  pub const HAIR: f64 = 2.0;
  /// Between an icon and its label, between segments.
  pub const XS: f64 = 4.0;
  /// Between related controls in a row; list row padding.
  pub const S: f64 = 8.0;
  /// Card padding; between groups in a bar.
  pub const M: f64 = 12.0;
  /// Between sections inside a card; overlay margins from the window edge.
  pub const L: f64 = 16.0;
}

/// Corner radii, in logical pixels.
pub mod radius {
  /// Buttons, fields, list rows.
  pub const CONTROL: f64 = 6.0;
  /// Cards, popovers, the command palette.
  pub const CARD: f64 = 10.0;
  /// Pills and chips: large enough to round any control fully.
  pub const PILL: f64 = 999.0;
}

/// The type scale, in logical pixels. Inter throughout.
pub mod text {
  /// The inspector's editable node name: the largest text in the chrome.
  pub const TITLE: f32 = 18.0;
  /// Body text: list rows, reason lines, popover items.
  pub const BODY: f32 = 14.0;
  /// Button and field labels.
  pub const CONTROL: f32 = 13.0;
  /// Secondary text: counts, hints, metadata.
  pub const SECONDARY: f32 = 12.0;
  /// Section labels (set in caps) and chips.
  pub const LABEL: f32 = 11.0;
}

/// Fixed chrome dimensions, in logical pixels.
pub mod size {
  /// Height of the top bar.
  pub const TOP_BAR: f64 = 44.0;
  /// Width of the drag handle along the inspector card's left edge.
  pub const DIVIDER: f64 = 6.0;
  /// Icon size inside a control.
  pub const ICON: f32 = 16.0;
}

/// Animation timing, in milliseconds.
pub mod motion {
  /// A popover dropping into place.
  pub const POPOVER_MS: f64 = 120.0;
  /// The inspector card sliding in.
  pub const CARD_MS: f64 = 160.0;
}
