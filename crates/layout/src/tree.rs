//! Which independent tree (weakly connected component) every node is in.

use std::collections::HashMap;

use base::{EdgeId, Graph, NodeId};

/// Disjoint sets over indices, each set's root its smallest member.
pub(crate) struct Forest {
  parent: Vec<usize>,
}

impl Forest {
  /// `n` singletons.
  fn new(n: usize) -> Self {
    Self {
      parent: (0..n).collect(),
    }
  }

  /// The root of `i`'s set, halving the path on the way.
  fn find(&mut self, mut i: usize) -> usize {
    while self.parent[i] != i {
      self.parent[i] = self.parent[self.parent[i]];
      i = self.parent[i];
    }
    i
  }

  /// Merge the sets of `a` and `b`. The smaller root stays the root, so a
  /// set's label is its smallest member.
  fn union(&mut self, a: usize, b: usize) {
    let (a, b) = (self.find(a), self.find(b));
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    self.parent[hi] = lo;
  }

  /// Label every node with its weakly connected component under the
  /// (acyclic) edge list, each tree labelled by its smallest node id's
  /// index so the labels are deterministic.
  pub(crate) fn label(
    graph: &Graph,
    edges: &[(EdgeId, NodeId, NodeId)],
  ) -> HashMap<NodeId, usize> {
    let mut nodes: Vec<NodeId> = graph.nodes().map(|n| n.id).collect();
    nodes.sort_unstable();
    let index: HashMap<NodeId, usize> =
      nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut forest = Forest::new(nodes.len());
    for (_, u, v) in edges {
      forest.union(index[u], index[v]);
    }
    nodes
      .iter()
      .enumerate()
      .map(|(i, n)| (*n, forest.find(i)))
      .collect()
  }
}
