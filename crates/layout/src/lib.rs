//! `layout` — pure Sugiyama-style layered-DAG layout for Neutron (PLAN §4).
//!
//! Given a [`base::Graph`], produce a deterministic set of node positions for
//! the canvas. First, [`copies`] decides which shared conditions are drawn
//! more than once, and the rest of the pipeline lays out that drawing, in
//! which every copy is a node of its own. Then comes the classic Sugiyama
//! sequence:
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
//!    own, with a wider gap between trees than between siblings, so separate
//!    trees never interleave. Within its column, [`coord`] (Brandes–Köpf) lines
//!    each node up with the median of its neighbours, so nodes sit over what
//!    they depend on and long edges run straight.
//!
//! Steps 1–3 depend only on the graph and produce an [`Arrangement`]; step 4
//! ([`Arrangement::place`]) also needs every node's size. They are separate
//! because sizes come from shaping label text, which only the UI can do: the
//! app arranges the graph, and the canvas places it once it has measured.
//!
//! The crate is pure: no I/O, no UI, no randomness. The same graph (and the
//! same sizes) always yields the same [`Layout`].

mod arrangement;
mod config;
mod coord;
mod copies;
mod cycle;
mod geom;
mod order;
mod rank;
mod tree;

use std::collections::{HashMap, HashSet};

use base::{EdgeId, Graph, NodeId};

use self::tree::Forest;
pub use self::{
  arrangement::{Arrangement, Placement, Slot},
  config::LayoutConfig,
  copies::Copies,
  geom::{Channel, Pos, Size},
};

/// The computed layout: a position and rank per drawn node, plus the edges
/// the cycle-cut heuristic reversed (which the UI styles as backward cycle
/// edges, PLAN §5).
///
/// Everything here is keyed by *drawn* node: a node itself, or one of the
/// extra copies [`Layout::copies`] maps back to it.
#[derive(Clone, Debug, Default)]
pub struct Layout {
  /// Canvas position of every drawn node, with every node at
  /// [`LayoutConfig::node_size`].
  pub positions:      HashMap<NodeId, Pos>,
  /// The rows the positions were placed from, to re-place with real sizes.
  pub arrangement:    Arrangement,
  /// Rank (row, 0 at the top) of every drawn node.
  pub ranks:          HashMap<NodeId, usize>,
  /// Edges reversed to break cycles.
  pub reversed_edges: HashSet<EdgeId>,
  /// Which drawn nodes are extra copies of a shared condition.
  pub copies:         Copies,
}

impl Layout {
  /// Lay the graph out with the given configuration, every node at
  /// [`LayoutConfig::node_size`].
  ///
  /// Splits shared conditions into their copies, then runs the full Sugiyama
  /// pipeline and assigns coordinates. Empty graphs
  /// yield an empty [`Layout`]. The result's [`Layout::arrangement`] can be
  /// re-placed once real node sizes are known.
  pub fn compute(graph: &Graph, cfg: &LayoutConfig) -> Layout {
    let (drawn, copies) = copies::split(graph);
    let graph = &drawn;
    let reversed = cycle::feedback_arcs(graph);
    let layering = rank::layer(graph, &reversed);
    let ordering = order::order(graph, &layering, cfg.barycenter_sweeps);
    // Every slot takes its tree's label; a bend belongs to its edge's tree.
    let node_tree = Forest::label(graph, &layering.dag_edges);
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
      links: ordering.links,
    };
    let positions = arrangement.place(cfg, |_| cfg.node_size).nodes;

    Layout {
      positions,
      arrangement,
      ranks: layering.ranks,
      reversed_edges: reversed,
      copies,
    }
  }

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
