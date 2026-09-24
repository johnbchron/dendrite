//! `layout` — pure Sugiyama-style layered-DAG layout for Neutron (PLAN §4).
//!
//! Given a [`base::Graph`], produce a deterministic set of node positions for
//! the canvas. The pipeline is the classic Sugiyama sequence:
//!
//! 1. [`cycle`] — feedback-arc cut so the graph can be layered.
//! 2. [`rank`] — top-down longest-path layering (goals at the top, requirements
//!    below, PLAN §7.3).
//! 3. [`order`] — within-level ordering seeded by `order_hint`, refined by
//!    barycenter crossing minimization (PLAN §5, risk §6.3). An edge that skips
//!    ranks gets a [`Slot::Bend`] in each rank it skips, so it is given a gap
//!    in every row it crosses instead of being drawn over a node.
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
  /// Width reserved for a long edge in each row it passes through (plus
  /// `x_gap` either side, like any other slot).
  pub bend_width:        f64,
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
      bend_width:        8.0,
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

/// One place in a row: a node, or the point where a long edge passes
/// through a rank it skips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
  /// A graph node.
  Node(NodeId),
  /// Where `edge` crosses row `rank`, strictly between its two ends.
  Bend {
    /// The edge passing through.
    edge: EdgeId,
    /// The rank it passes through.
    rank: usize,
  },
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

/// Coordinates for an [`Arrangement`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
  /// Centre of every node.
  pub nodes:    HashMap<NodeId, Pos>,
  /// For every edge that skips ranks, the channels it runs through, top row
  /// first. Edges between adjacent ranks have none.
  pub channels: HashMap<EdgeId, Vec<Channel>>,
}

/// The graph's shape before coordinates: which row every node (and every
/// long edge's bend) sits in, in what left-to-right order, and which tree it
/// belongs to. Everything [`Arrangement::place`] needs besides node sizes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
  /// Slots per rank, top row first, each in left-to-right order.
  pub rows: Vec<Vec<Slot>>,
  /// The independent tree (weakly connected component) of every slot, as an
  /// opaque label: slots share a label exactly when edges connect them.
  pub tree: HashMap<Slot, usize>,
}

impl Arrangement {
  /// The same arrangement with only the slots `keep` accepts, in the same
  /// rows and order. Rows left without a node are dropped, so a filtered
  /// view does not keep a blank band for a rank it hides entirely.
  ///
  /// Callers should drop a bend whenever they drop either end of its edge.
  pub fn retain(&self, keep: impl Fn(Slot) -> bool) -> Arrangement {
    let rows = self
      .rows
      .iter()
      .map(|row| row.iter().copied().filter(|s| keep(*s)).collect::<Vec<_>>())
      .filter(|row| row.iter().any(|s| matches!(s, Slot::Node(_))))
      .collect();
    Arrangement {
      rows,
      tree: self.tree.clone(),
    }
  }

  /// Assign every node a centre position, given its size, and every long
  /// edge the channels it runs through.
  ///
  /// Each row is as tall as its tallest node, with nodes centred on the
  /// row's midline, and rows are `y_gap` apart; rows line up across trees.
  /// Each tree takes a column as wide as its widest row, trees are
  /// `tree_gap` apart (left to right in order of first appearance, reading
  /// the rows top-down), and the whole is centred on x = 0. Within a column,
  /// slots are `x_gap` apart and every row is centred; a bend is
  /// `bend_width` wide.
  pub fn place(
    &self,
    cfg: &LayoutConfig,
    size_of: impl Fn(NodeId) -> Size,
  ) -> Placement {
    let width_of = |slot: Slot| match slot {
      Slot::Node(n) => size_of(n).w,
      Slot::Bend { .. } => cfg.bend_width,
    };

    // Trees in order of first appearance; the barycenter sweeps put related
    // nodes near each other, so this keeps the arrangement's reading order.
    let mut trees: Vec<usize> = Vec::new();
    for slot in self.rows.iter().flatten() {
      let t = self.tree_of(*slot);
      if !trees.contains(&t) {
        trees.push(t);
      }
    }

    // Each tree's slice of each row, keeping the row's order.
    let slices: Vec<Vec<Vec<Slot>>> = trees
      .iter()
      .map(|&t| {
        self
          .rows
          .iter()
          .map(|row| {
            row
              .iter()
              .copied()
              .filter(|s| self.tree_of(*s) == t)
              .collect()
          })
          .collect()
      })
      .collect();
    let row_width = |row: &[Slot]| {
      row.iter().map(|s| width_of(*s)).sum::<f64>()
        + cfg.x_gap * row.len().saturating_sub(1) as f64
    };
    let widths: Vec<f64> = slices
      .iter()
      .map(|rows| rows.iter().map(|r| row_width(r)).fold(0.0, f64::max))
      .collect();
    let total = widths.iter().sum::<f64>()
      + cfg.tree_gap * trees.len().saturating_sub(1) as f64;

    // Row bands, shared by every tree: (top, height). Bends have no height
    // of their own; they span whatever band the row's nodes make.
    let mut bands = Vec::with_capacity(self.rows.len());
    let mut top = 0.0;
    for row in &self.rows {
      let height = row
        .iter()
        .filter_map(|s| match s {
          Slot::Node(n) => Some(size_of(*n).h),
          Slot::Bend { .. } => None,
        })
        .fold(0.0, f64::max);
      bands.push((top, height));
      top += height + cfg.y_gap;
    }

    let mut placement = Placement::default();
    let mut column_left = -total / 2.0;
    for (rows, width) in slices.iter().zip(&widths) {
      let centre = column_left + width / 2.0;
      for (row, &(top, height)) in rows.iter().zip(&bands) {
        let mut left = centre - row_width(row) / 2.0;
        for slot in row {
          let w = width_of(*slot);
          let x = left + w / 2.0;
          match *slot {
            Slot::Node(n) => {
              placement.nodes.insert(n, Pos {
                x,
                y: top + height / 2.0,
              });
            }
            // Rows are walked top-down, so each edge's channels come out
            // top row first.
            Slot::Bend { edge, .. } => {
              placement.channels.entry(edge).or_default().push(Channel {
                x,
                top,
                bottom: top + height,
              });
            }
          }
          left += w + cfg.x_gap;
        }
      }
      column_left += width + cfg.tree_gap;
    }
    placement
  }

  /// The tree label of `slot`. A slot missing from [`Self::tree`] stands
  /// alone, under a label no real tree uses.
  fn tree_of(&self, slot: Slot) -> usize {
    self.tree.get(&slot).copied().unwrap_or(usize::MAX)
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
  // Every slot takes its tree's label; a bend belongs to its edge's tree.
  let node_tree = trees(graph, &layering.dag_edges);
  let tree = ordering
    .by_rank
    .iter()
    .flatten()
    .map(|&slot| {
      let owner = match slot {
        Slot::Node(n) => n,
        Slot::Bend { edge, .. } => {
          graph.edge(edge).expect("bends come from graph edges").from
        }
      };
      (slot, node_tree[&owner])
    })
    .collect();
  let arrangement = Arrangement {
    tree,
    rows: ordering.by_rank,
  };
  let positions = arrangement.place(cfg, |_| cfg.node_size).nodes;

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
fn trees(
  graph: &Graph,
  edges: &[(EdgeId, NodeId, NodeId)],
) -> HashMap<NodeId, usize> {
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
  for (_, u, v) in edges {
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
