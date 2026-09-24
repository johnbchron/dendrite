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
//! 4. coordinate assignment — each rank a row as tall as its tallest node.
//!    Every independent tree (weakly connected component) gets a column of its
//!    own, as wide as its widest row, with a wider gap between trees than
//!    between siblings, so separate trees never interleave; within its column
//!    each row is centred.
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
  /// Horizontal gap between independent trees. Wider than `x_gap`, so the
  /// space between two trees reads differently from the space within one.
  pub tree_gap:          f64,
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
      tree_gap:          64.0,
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

/// The graph's shape before coordinates: which row every node sits in, in
/// what left-to-right order, and which tree it belongs to. Everything
/// [`Arrangement::place`] needs besides node sizes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
  /// Node ids per rank, top row first, each in left-to-right order.
  pub rows: Vec<Vec<NodeId>>,
  /// The independent tree (weakly connected component) of every node, as an
  /// opaque label: nodes share a label exactly when edges connect them.
  pub tree: HashMap<NodeId, usize>,
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
    Arrangement {
      rows,
      tree: self.tree.clone(),
    }
  }

  /// Assign every node a centre position, given its size.
  ///
  /// Each row is as tall as its tallest node, with nodes centred on the
  /// row's midline, and rows are `y_gap` apart; rows line up across trees.
  /// Each tree takes a column as wide as its widest row, trees are
  /// `tree_gap` apart (left to right in order of first appearance, reading
  /// the rows top-down), and the whole is centred on x = 0. Within a column,
  /// boxes are `x_gap` apart and every row is centred.
  pub fn place(
    &self,
    cfg: &LayoutConfig,
    size_of: impl Fn(NodeId) -> Size,
  ) -> HashMap<NodeId, Pos> {
    // Trees in order of first appearance; the barycenter sweeps put related
    // nodes near each other, so this keeps the arrangement's reading order.
    let mut trees: Vec<usize> = Vec::new();
    for node in self.rows.iter().flatten() {
      let t = self.tree_of(*node);
      if !trees.contains(&t) {
        trees.push(t);
      }
    }

    // Each tree's slice of each row, keeping the row's order.
    let slices: Vec<Vec<Vec<NodeId>>> = trees
      .iter()
      .map(|&t| {
        self
          .rows
          .iter()
          .map(|row| {
            row
              .iter()
              .copied()
              .filter(|n| self.tree_of(*n) == t)
              .collect()
          })
          .collect()
      })
      .collect();
    let row_width = |row: &[NodeId]| {
      row.iter().map(|n| size_of(*n).w).sum::<f64>()
        + cfg.x_gap * row.len().saturating_sub(1) as f64
    };
    let widths: Vec<f64> = slices
      .iter()
      .map(|rows| rows.iter().map(|r| row_width(r)).fold(0.0, f64::max))
      .collect();
    let total = widths.iter().sum::<f64>()
      + cfg.tree_gap * trees.len().saturating_sub(1) as f64;

    // Row bands, shared by every tree.
    let mut mids = Vec::with_capacity(self.rows.len());
    let mut top = 0.0;
    for row in &self.rows {
      let height = row.iter().map(|n| size_of(*n).h).fold(0.0, f64::max);
      mids.push(top + height / 2.0);
      top += height + cfg.y_gap;
    }

    let mut positions = HashMap::new();
    let mut column_left = -total / 2.0;
    for (rows, width) in slices.iter().zip(&widths) {
      let centre = column_left + width / 2.0;
      for (row, &y) in rows.iter().zip(&mids) {
        let mut left = centre - row_width(row) / 2.0;
        for node in row {
          let w = size_of(*node).w;
          positions.insert(*node, Pos {
            x: left + w / 2.0,
            y,
          });
          left += w + cfg.x_gap;
        }
      }
      column_left += width + cfg.tree_gap;
    }
    positions
  }

  /// The tree label of `node`. A node missing from [`Self::tree`] stands
  /// alone, under a label no real tree uses.
  fn tree_of(&self, node: NodeId) -> usize {
    self.tree.get(&node).copied().unwrap_or(usize::MAX)
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
    tree: trees(graph, &layering.dag_edges),
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

/// Label every node with its weakly connected component: union-find over
/// the (acyclic) edge list, each tree labelled by its smallest node id's
/// index so the labels are deterministic.
fn trees(graph: &Graph, edges: &[(NodeId, NodeId)]) -> HashMap<NodeId, usize> {
  let mut nodes: Vec<NodeId> = graph.nodes().map(|n| n.id).collect();
  nodes.sort_unstable();
  let index: HashMap<NodeId, usize> =
    nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
  let mut parent: Vec<usize> = (0..nodes.len()).collect();
  fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
      parent[i] = parent[parent[i]];
      i = parent[i];
    }
    i
  }
  for (u, v) in edges {
    let (a, b) = (find(&mut parent, index[u]), find(&mut parent, index[v]));
    // Keep the smaller index as the root, so a tree's label is its
    // smallest member.
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    parent[hi] = lo;
  }
  nodes
    .iter()
    .enumerate()
    .map(|(i, n)| (*n, find(&mut parent, i)))
    .collect()
}
