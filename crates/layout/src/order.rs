//! Within-level ordering and crossing minimization (PLAN §4 step 3, §5).
//!
//! Each rank's nodes are first placed in the user's preferred order — sorted
//! by [`order_hint`](base::Node::order_hint) — so a chosen ordering survives
//! relayout (risk §6.3). A few barycenter sweeps then reduce edge crossings
//! while leaving unconstrained nodes where the hint put them.

use std::collections::HashMap;

use base::{Graph, NodeId};

use crate::rank::Layering;

/// The nodes of every rank in their final left-to-right order.
///
/// `by_rank[r]` is rank `r`'s ordered node list; a node's x index is its
/// position in that vector.
pub struct Ordering {
  /// Ordered node ids per rank.
  pub by_rank: Vec<Vec<NodeId>>,
}

/// Order every rank, seeding from `order_hint` then applying `sweeps`
/// barycenter passes (alternating down and up).
pub fn order(graph: &Graph, layering: &Layering, sweeps: u32) -> Ordering {
  let max_rank = layering.ranks.values().copied().max().unwrap_or(0);
  let mut by_rank: Vec<Vec<NodeId>> = vec![Vec::new(); max_rank + 1];
  for (&node, &rank) in &layering.ranks {
    by_rank[rank].push(node);
  }

  // Seed: ascending order_hint, node id breaking ties. Deterministic.
  for row in &mut by_rank {
    row.sort_by(|a, b| {
      let ha = graph.node(*a).map(|n| n.order_hint).unwrap_or(0.0);
      let hb = graph.node(*b).map(|n| n.order_hint).unwrap_or(0.0);
      ha.total_cmp(&hb).then(a.cmp(b))
    });
  }

  // Upper/lower adjacency from the acyclic edge list.
  let mut upper: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
  let mut lower: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
  for &(u, v) in &layering.dag_edges {
    lower.entry(u).or_default().push(v);
    upper.entry(v).or_default().push(u);
  }

  let mut index = index_map(&by_rank);
  for sweep in 0..sweeps {
    if sweep % 2 == 0 {
      // Down: order each rank by the barycenter of its upper neighbours.
      for r in 1..=max_rank {
        reorder_rank(&mut by_rank[r], &upper, &index);
        refresh(&by_rank[r], &mut index);
      }
    } else {
      // Up: order each rank by the barycenter of its lower neighbours.
      for r in (0..max_rank).rev() {
        reorder_rank(&mut by_rank[r], &lower, &index);
        refresh(&by_rank[r], &mut index);
      }
    }
  }

  Ordering { by_rank }
}

/// Map each node to its current within-rank index.
fn index_map(by_rank: &[Vec<NodeId>]) -> HashMap<NodeId, usize> {
  let mut m = HashMap::new();
  for row in by_rank {
    for (i, n) in row.iter().enumerate() {
      m.insert(*n, i);
    }
  }
  m
}

/// Refresh the indices of a single rank after it was reordered.
fn refresh(row: &[NodeId], index: &mut HashMap<NodeId, usize>) {
  for (i, n) in row.iter().enumerate() {
    index.insert(*n, i);
  }
}

/// Stable-sort one rank by the barycenter of each node's neighbours in
/// `adj`. A node with no neighbours keeps its current index, so unconstrained
/// nodes stay where the order-hint seed put them.
fn reorder_rank(
  row: &mut [NodeId],
  adj: &HashMap<NodeId, Vec<NodeId>>,
  index: &HashMap<NodeId, usize>,
) {
  // Snapshot current indices so "no neighbours → stay put" is well defined
  // even as we sort.
  let current: HashMap<NodeId, usize> =
    row.iter().enumerate().map(|(i, n)| (*n, i)).collect();
  let bary = |n: &NodeId| -> f64 {
    match adj.get(n) {
      Some(neigh) if !neigh.is_empty() => {
        let sum: usize = neigh.iter().map(|m| index[m]).sum();
        sum as f64 / neigh.len() as f64
      }
      _ => current[n] as f64,
    }
  };
  // Stable sort keeps the seeded order for equal barycenters.
  row.sort_by(|a, b| bary(a).total_cmp(&bary(b)));
}
