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
//! 4. coordinate assignment — each rank a row as tall as its tallest node,
//!    nodes a fixed gap apart within it, every row centred on a shared axis.
//!
//! Steps 1–3 depend only on the graph and produce an [`Arrangement`]; step 4
//! ([`Arrangement::place`]) also needs every node's size. They are separate
//! because sizes come from shaping label text, which only the UI can do: the
//! app arranges the graph, and the canvas places it once it has measured.
//!
//! The crate is pure: no I/O, no UI, no randomness. The same graph (and the
//! same sizes) always yields the same [`Layout`].

mod cycle;
mod order;
mod rank;

use std::collections::{HashMap, HashSet};

use base::{EdgeId, Graph, NodeId};

/// Tunable spacing and effort for a layout run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutConfig {
  /// Horizontal gap between the boxes of adjacent nodes in a rank.
  pub x_gap:             f64,
  /// Vertical gap between the bottom of one rank and the top of the next.
  pub y_gap:             f64,
  /// Node size [`layout`] assumes when no measured sizes are given.
  pub node_size:         Size,
  /// Number of barycenter sweeps (alternating down/up) for crossing
  /// minimization. Zero leaves the order-hint seed untouched.
  pub barycenter_sweeps: u32,
}

impl Default for LayoutConfig {
  fn default() -> Self {
    Self {
      x_gap:             16.0,
      y_gap:             56.0,
      node_size:         Size { w: 150.0, h: 46.0 },
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

/// The width and height of a node's box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
  /// Horizontal extent.
  pub w: f64,
  /// Vertical extent.
  pub h: f64,
}

/// The graph's shape before coordinates: which row every node sits in and
/// in what left-to-right order. Everything [`Arrangement::place`] needs
/// besides node sizes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
  /// Node ids per rank, top row first, each in left-to-right order.
  pub rows: Vec<Vec<NodeId>>,
}

impl Arrangement {
  /// The same arrangement with only the nodes `keep` accepts, in the same
  /// rows and order. Rows left empty are dropped, so a filtered view does
  /// not keep a blank band for a rank it hides entirely.
  pub fn retain(&self, keep: impl Fn(NodeId) -> bool) -> Arrangement {
    let rows = self
      .rows
      .iter()
      .map(|row| row.iter().copied().filter(|n| keep(*n)).collect::<Vec<_>>())
      .filter(|row| !row.is_empty())
      .collect();
    Arrangement { rows }
  }

  /// Assign every node a centre position, given its size.
  ///
  /// Each row is as tall as its tallest node, with nodes centred on the
  /// row's midline, and rows are `y_gap` apart. Within a row, boxes are
  /// `x_gap` apart and the row is centred on the shared x = 0 axis.
  pub fn place(
    &self,
    cfg: &LayoutConfig,
    size_of: impl Fn(NodeId) -> Size,
  ) -> HashMap<NodeId, Pos> {
    let mut positions = HashMap::new();
    let mut top = 0.0;
    for row in &self.rows {
      let sizes: Vec<Size> = row.iter().map(|n| size_of(*n)).collect();
      let height = sizes.iter().map(|s| s.h).fold(0.0, f64::max);
      let width = sizes.iter().map(|s| s.w).sum::<f64>()
        + cfg.x_gap * (row.len().saturating_sub(1)) as f64;
      let y = top + height / 2.0;
      let mut left = -width / 2.0;
      for (node, size) in row.iter().zip(&sizes) {
        positions.insert(*node, Pos {
          x: left + size.w / 2.0,
          y,
        });
        left += size.w + cfg.x_gap;
      }
      top += height + cfg.y_gap;
    }
    positions
  }
}

/// The computed layout: a position and rank per node, plus the edges the
/// cycle-cut heuristic reversed (which the UI styles as backward cycle
/// edges, PLAN §5).
#[derive(Clone, Debug, Default)]
pub struct Layout {
  /// Canvas position of every node, with every node at
  /// [`LayoutConfig::node_size`].
  pub positions:      HashMap<NodeId, Pos>,
  /// The rows the positions were placed from, to re-place with real sizes.
  pub arrangement:    Arrangement,
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

/// Lay the graph out with the given configuration, every node at
/// [`LayoutConfig::node_size`].
///
/// Runs the full Sugiyama pipeline and assigns coordinates. Empty graphs
/// yield an empty [`Layout`]. The result's [`Layout::arrangement`] can be
/// re-placed once real node sizes are known.
pub fn layout(graph: &Graph, cfg: &LayoutConfig) -> Layout {
  let reversed = cycle::feedback_arcs(graph);
  let layering = rank::layer(graph, &reversed);
  let ordering = order::order(graph, &layering, cfg.barycenter_sweeps);
  let arrangement = Arrangement {
    rows: ordering.by_rank,
  };
  let positions = arrangement.place(cfg, |_| cfg.node_size);

  Layout {
    positions,
    arrangement,
    ranks: layering.ranks,
    reversed_edges: reversed,
  }
}
