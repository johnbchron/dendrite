//! Within-level ordering and crossing minimization (PLAN §4 step 3, §5).
//!
//! Each rank's nodes are first placed in the user's preferred order — sorted
//! by [`order_hint`](base::Node::order_hint) — so a chosen ordering survives
//! relayout (risk §6.3). A few barycenter sweeps then reduce edge crossings
//! while leaving unconstrained nodes where the hint put them.
//!
//! An edge that spans several ranks is split into unit-length segments by a
//! [`Slot::Bend`] in every rank it skips. The bends are ordered along with
//! the nodes, so each one claims a gap in its row for the edge to pass
//! through rather than being drawn across whatever node sits there.

use std::collections::HashMap;

use base::{Graph, NodeId};

use crate::{Slot, rank::Layering};

/// The slots of every rank in their final left-to-right order.
///
/// `by_rank[r]` is rank `r`'s ordered slot list; a slot's x index is its
/// position in that vector.
pub struct Ordering {
  /// Ordered slots per rank.
  pub by_rank: Vec<Vec<Slot>>,
  /// Every unit-length segment, upper slot first: each edge between
  /// adjacent ranks, and each piece of a long edge chained through its
  /// bends.
  pub links: Vec<(Slot, Slot)>,
}

/// Order every rank, seeding from `order_hint` then applying `sweeps`
/// barycenter passes (alternating down and up).
pub fn order(graph: &Graph, layering: &Layering, sweeps: u32) -> Ordering {
  let max_rank = layering.ranks.values().copied().max().unwrap_or(0);
  let hint = |n: NodeId| graph.node(n).map(|n| n.order_hint).unwrap_or(0.0);

  // Seed keys: a node's own order_hint; a bend sits between the hints of
  // its edge's ends, so it starts out roughly under the path it belongs to.
  let mut seeded: Vec<Vec<(f64, Slot)>> = vec![Vec::new(); max_rank + 1];
  for (&node, &rank) in &layering.ranks {
    seeded[rank].push((hint(node), Slot::Node(node)));
  }

  // Upper/lower adjacency over unit-length segments: every long edge is
  // chained through one bend per rank it skips.
  let mut upper: HashMap<Slot, Vec<Slot>> = HashMap::new();
  let mut lower: HashMap<Slot, Vec<Slot>> = HashMap::new();
  let mut links = Vec::new();
  let mut link = |u: Slot, v: Slot| {
    links.push((u, v));
    lower.entry(u).or_default().push(v);
    upper.entry(v).or_default().push(u);
  };
  for &(edge, u, v) in &layering.dag_edges {
    let (ru, rv) = (layering.ranks[&u], layering.ranks[&v]);
    let key = (hint(u) + hint(v)) / 2.0;
    let mut prev = Slot::Node(u);
    for rank in ru + 1..rv {
      let bend = Slot::Bend { edge, rank };
      seeded[rank].push((key, bend));
      link(prev, bend);
      prev = bend;
    }
    link(prev, Slot::Node(v));
  }

  // Seed order: ascending key, slot breaking ties. Deterministic.
  let mut by_rank: Vec<Vec<Slot>> = seeded
    .into_iter()
    .map(|mut row| {
      row.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
      row.into_iter().map(|(_, slot)| slot).collect()
    })
    .collect();

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

  Ordering { by_rank, links }
}

/// Map each slot to its current within-rank index.
fn index_map(by_rank: &[Vec<Slot>]) -> HashMap<Slot, usize> {
  let mut m = HashMap::new();
  for row in by_rank {
    for (i, n) in row.iter().enumerate() {
      m.insert(*n, i);
    }
  }
  m
}

/// Refresh the indices of a single rank after it was reordered.
fn refresh(row: &[Slot], index: &mut HashMap<Slot, usize>) {
  for (i, n) in row.iter().enumerate() {
    index.insert(*n, i);
  }
}

/// Stable-sort one rank by the barycenter of each slot's neighbours in
/// `adj`. A slot with no neighbours keeps its current index, so
/// unconstrained nodes stay where the order-hint seed put them.
fn reorder_rank(
  row: &mut [Slot],
  adj: &HashMap<Slot, Vec<Slot>>,
  index: &HashMap<Slot, usize>,
) {
  // Snapshot current indices so "no neighbours → stay put" is well defined
  // even as we sort.
  let current: HashMap<Slot, usize> =
    row.iter().enumerate().map(|(i, n)| (*n, i)).collect();
  let bary = |n: &Slot| -> f64 {
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
