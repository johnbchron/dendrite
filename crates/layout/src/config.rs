//! Tunable spacing and effort.

use crate::Size;

/// Tunable spacing and effort for a layout run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfig {
  /// Horizontal gap between the boxes of adjacent nodes in a rank.
  pub x_gap: f64,
  /// Vertical gap between the bottom of one rank and the top of the next.
  pub y_gap: f64,
  /// Horizontal gap between independent trees. Wider than `x_gap`, so the
  /// space between two trees reads differently from the space within one.
  pub tree_gap: f64,
  /// Node size [`Layout::compute`](crate::Layout::compute) assumes when no
  /// measured sizes are given.
  pub node_size: Size,
  /// Width reserved for a long edge in each row it passes through (plus
  /// `x_gap` either side, like any other slot).
  pub bend_width: f64,
  /// Number of barycenter sweeps (alternating down/up) for crossing
  /// minimization. Zero leaves the order-hint seed untouched.
  pub barycenter_sweeps: u32,
}

impl Default for LayoutConfig {
  fn default() -> Self {
    Self {
      x_gap: 16.0,
      y_gap: 56.0,
      tree_gap: 64.0,
      node_size: Size { w: 150.0, h: 46.0 },
      bend_width: 8.0,
      barycenter_sweeps: 4,
    }
  }
}
