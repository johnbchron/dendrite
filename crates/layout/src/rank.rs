//! Longest-path layering, top-down (PLAN §4 step 2, §7.3).
//!
//! An edge reads "`from` requires `to`", so the dependent/goal `from` sits
//! *above* its requirement `to`. Ranks therefore increase downward:
//! `rank(to) >= rank(from) + 1`. Goals — nodes nothing requires — have no
//! incoming requirement and land at rank 0 at the top. Reversed edges (the
//! feedback arcs from [`crate::cycle`]) are treated as flowing the other way
//! so the layering sees a DAG.

use std::collections::HashMap;

use base::{EdgeId, Graph, NodeId};
use petgraph::{algo::toposort, graphmap::DiGraphMap};

/// A layering result: a rank per node and the acyclic edge list (with
/// feedback arcs already flipped) used by the ordering stage.
pub struct Layering {
  /// Rank (row index, 0 at the top) for every node.
  pub ranks:     HashMap<NodeId, usize>,
  /// DAG edges as `(edge, upper, lower)` — `upper` sits one-or-more ranks
  /// above `lower`. Self-loops are excluded.
  pub dag_edges: Vec<(EdgeId, NodeId, NodeId)>,
}

/// Layer the graph, given the set of edges that must be reversed to make it
/// acyclic.
pub fn layer(
  graph: &Graph,
  reversed: &std::collections::HashSet<EdgeId>,
) -> Layering {
  // Build the acyclic edge list: a reversed edge flows `to -> from`, an
  // ordinary edge flows `from -> to`. Self-loops carry no layering signal.
  let mut dag_edges: Vec<(EdgeId, NodeId, NodeId)> = Vec::new();
  for e in graph.edges() {
    let (upper, lower) = if reversed.contains(&e.id) {
      (e.to, e.from)
    } else {
      (e.from, e.to)
    };
    if upper != lower {
      dag_edges.push((e.id, upper, lower));
    }
  }

  // Longest-path ranks, relaxed in topological order: any order gives the
  // same ranks, so the result is deterministic.
  let mut dag = DiGraphMap::<NodeId, ()>::new();
  for n in graph.nodes() {
    dag.add_node(n.id);
  }
  for &(_, u, v) in &dag_edges {
    dag.add_edge(u, v, ());
  }
  let mut ranks: HashMap<NodeId, usize> = dag.nodes().map(|n| (n, 0)).collect();
  // Defensive: if a residual cycle survived the cut (it should not), every
  // node keeps rank 0 rather than being dropped.
  let topo = toposort(&dag, None);
  debug_assert!(topo.is_ok(), "cut left a residual cycle");
  for u in topo.unwrap_or_default() {
    let next = ranks[&u] + 1;
    for v in dag.neighbors(u) {
      let rv = ranks.get_mut(&v).unwrap();
      *rv = (*rv).max(next);
    }
  }

  Layering { ranks, dag_edges }
}
