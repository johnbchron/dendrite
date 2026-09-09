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

/// A layering result: a rank per node and the acyclic edge list (with
/// feedback arcs already flipped) used by the ordering stage.
pub struct Layering {
  /// Rank (row index, 0 at the top) for every node.
  pub ranks:     HashMap<NodeId, usize>,
  /// DAG edges as `(upper, lower)` pairs — `upper` sits one-or-more ranks
  /// above `lower`. Self-loops are excluded.
  pub dag_edges: Vec<(NodeId, NodeId)>,
}

/// Layer the graph, given the set of edges that must be reversed to make it
/// acyclic.
pub fn layer(
  graph: &Graph,
  reversed: &std::collections::HashSet<EdgeId>,
) -> Layering {
  // Build the acyclic edge list: a reversed edge flows `to -> from`, an
  // ordinary edge flows `from -> to`. Self-loops carry no layering signal.
  let mut dag_edges: Vec<(NodeId, NodeId)> = Vec::new();
  for e in graph.edges() {
    let (upper, lower) = if reversed.contains(&e.id) {
      (e.to, e.from)
    } else {
      (e.from, e.to)
    };
    if upper != lower {
      dag_edges.push((upper, lower));
    }
  }

  // Kahn's algorithm carrying a longest-path rank. Successor adjacency and
  // in-degrees are keyed per node; the ready queue is drained in sorted-id
  // order so the traversal is deterministic.
  let mut succ: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
  let mut indeg: HashMap<NodeId, usize> = HashMap::new();
  let mut nodes: Vec<NodeId> = graph.nodes().map(|n| n.id).collect();
  nodes.sort_unstable();
  for &n in &nodes {
    indeg.entry(n).or_insert(0);
    succ.entry(n).or_default();
  }
  for &(u, v) in &dag_edges {
    succ.entry(u).or_default().push(v);
    *indeg.entry(v).or_insert(0) += 1;
  }

  let mut ranks: HashMap<NodeId, usize> =
    nodes.iter().map(|&n| (n, 0usize)).collect();
  let mut ready: Vec<NodeId> =
    nodes.iter().copied().filter(|n| indeg[n] == 0).collect();
  ready.sort_unstable();

  let mut processed = 0usize;
  while let Some(u) = pop_min(&mut ready) {
    processed += 1;
    let ru = ranks[&u];
    // Deterministic successor order.
    let mut children = succ[&u].clone();
    children.sort_unstable();
    for v in children {
      let nr = ru + 1;
      if nr > ranks[&v] {
        ranks.insert(v, nr);
      }
      let d = indeg.get_mut(&v).unwrap();
      *d -= 1;
      if *d == 0 {
        insert_sorted(&mut ready, v);
      }
    }
  }

  // Defensive: if a residual cycle survived the cut (it should not), any
  // unprocessed node keeps rank 0 rather than being dropped.
  debug_assert_eq!(processed, nodes.len(), "cut left a residual cycle");
  let _ = processed;

  Layering { ranks, dag_edges }
}

/// Pop the smallest id from a set kept in ascending order.
fn pop_min(v: &mut Vec<NodeId>) -> Option<NodeId> {
  if v.is_empty() {
    None
  } else {
    Some(v.remove(0))
  }
}

/// Insert keeping ascending order (the queue stays small in practice).
fn insert_sorted(v: &mut Vec<NodeId>, x: NodeId) {
  let pos = v.partition_point(|e| *e < x);
  v.insert(pos, x);
}
