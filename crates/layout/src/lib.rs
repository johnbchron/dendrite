//! `layout` — pure Sugiyama-style layered-DAG layout for Neutron (PLAN §4).
//!
//! Given a [`base::Graph`], produce a deterministic set of node positions for
//! the canvas. The pipeline is the classic Sugiyama sequence:
//!
//! 1. [`cycle`] — feedback-arc cut so the graph can be layered.
//! 2. [`rank`] — top-down longest-path layering (goals at the top, requirements
//!    below, PLAN §7.3).
//! 3. [`order`] — within-level ordering seeded by `order_hint`, refined by
//!    barycenter crossing minimization (PLAN §5, risk §6.3).
//! 4. x-coordinate assignment — even, stable spacing within each rank.
//!
//! The crate is pure: no I/O, no UI, no randomness. The same graph always
//! yields the same [`Layout`].

mod cycle;
mod order;
mod rank;

use std::collections::{HashMap, HashSet};

use base::{EdgeId, Graph, NodeId};

/// Tunable spacing and effort for a layout run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfig {
  /// Horizontal distance between adjacent nodes in a rank.
  pub x_spacing:         f64,
  /// Vertical distance between ranks.
  pub y_spacing:         f64,
  /// Number of barycenter sweeps (alternating down/up) for crossing
  /// minimization. Zero leaves the order-hint seed untouched.
  pub barycenter_sweeps: u32,
}

impl Default for LayoutConfig {
  fn default() -> Self {
    Self {
      x_spacing:         160.0,
      y_spacing:         110.0,
      barycenter_sweeps: 4,
    }
  }
}

/// A 2-D position on the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pos {
  /// Horizontal coordinate.
  pub x: f64,
  /// Vertical coordinate (increases downward).
  pub y: f64,
}

/// The computed layout: a position and rank per node, plus the edges the
/// cycle-cut heuristic reversed (which the UI styles as backward cycle
/// edges, PLAN §5).
#[derive(Clone, Debug, Default)]
pub struct Layout {
  /// Canvas position of every node.
  pub positions:      HashMap<NodeId, Pos>,
  /// Rank (row, 0 at the top) of every node.
  pub ranks:          HashMap<NodeId, usize>,
  /// Edges reversed to break cycles.
  pub reversed_edges: HashSet<EdgeId>,
}

impl Layout {
  /// Position of `node`, if it was laid out.
  pub fn pos(&self, node: NodeId) -> Option<Pos> {
    self.positions.get(&node).copied()
  }

  /// Rank of `node`, if it was laid out.
  pub fn rank(&self, node: NodeId) -> Option<usize> {
    self.ranks.get(&node).copied()
  }

  /// Whether `edge` was reversed by the cycle cut.
  pub fn is_reversed(&self, edge: EdgeId) -> bool {
    self.reversed_edges.contains(&edge)
  }
}

/// Lay the graph out with the given configuration.
///
/// Runs the full Sugiyama pipeline and assigns coordinates. Empty graphs
/// yield an empty [`Layout`].
pub fn layout(graph: &Graph, cfg: &LayoutConfig) -> Layout {
  let reversed = cycle::feedback_arcs(graph);
  let layering = rank::layer(graph, &reversed);
  let ordering = order::order(graph, &layering, cfg.barycenter_sweeps);

  let mut positions = HashMap::with_capacity(graph.node_count());
  for (rank, row) in ordering.by_rank.iter().enumerate() {
    for (col, node) in row.iter().enumerate() {
      positions.insert(*node, Pos {
        x: col as f64 * cfg.x_spacing,
        y: rank as f64 * cfg.y_spacing,
      });
    }
  }

  Layout {
    positions,
    ranks: layering.ranks,
    reversed_edges: reversed,
  }
}
