//! Which independent tree (weakly connected component) every node is in,
//! and which column each tree takes.

use std::collections::HashMap;

use base::{EdgeId, Graph, NodeId};
use petgraph::unionfind::UnionFind;

/// Label every node with its weakly connected component under the
/// (acyclic) edge list, numbered by the column the component takes,
/// counting from the left.
///
/// Columns go by each tree's lowest order hint, then its lowest id: a key
/// that an edit inside a tree rarely changes, since new nodes are hinted
/// after their siblings (see [`Graph::hint_after`]). So trees keep their
/// columns across edits that neither join nor split them, rather than
/// trading places whenever the crossing sweeps reorder the top row
/// (PLAN §6.3).
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
  let component = forest.into_labeling();

  // Each component's key: its lowest hint, then its lowest id. Nodes are
  // walked in id order, so the first id seen is the lowest.
  let hint = |n: NodeId| graph.node(n).map_or(0.0, |n| n.order_hint);
  let mut keys: HashMap<usize, (f64, NodeId)> = HashMap::new();
  for (&node, &c) in nodes.iter().zip(&component) {
    let key = keys.entry(c).or_insert((hint(node), node));
    key.0 = key.0.min(hint(node));
  }
  let mut order: Vec<(usize, (f64, NodeId))> = keys.into_iter().collect();
  order.sort_by(|(_, a), (_, b)| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
  let column: HashMap<usize, usize> = order
    .iter()
    .enumerate()
    .map(|(i, (c, _))| (*c, i))
    .collect();

  nodes
    .into_iter()
    .zip(component)
    .map(|(n, c)| (n, column[&c]))
    .collect()
}
