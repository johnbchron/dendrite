//! Which independent tree (weakly connected component) every node is in.

use std::collections::HashMap;

use base::{EdgeId, Graph, NodeId};
use petgraph::unionfind::UnionFind;

/// Label every node with its weakly connected component under the
/// (acyclic) edge list. The labels are opaque: only which nodes share one
/// matters.
pub(crate) fn label(
  graph: &Graph,
  edges: &[(EdgeId, NodeId, NodeId)],
) -> HashMap<NodeId, usize> {
  let mut nodes: Vec<NodeId> = graph.nodes().map(|n| n.id).collect();
  nodes.sort_unstable();
  let index: HashMap<NodeId, usize> =
    nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
  let mut forest = UnionFind::new(nodes.len());
  for (_, u, v) in edges {
    forest.union(index[u], index[v]);
  }
  nodes.into_iter().zip(forest.into_labeling()).collect()
}
