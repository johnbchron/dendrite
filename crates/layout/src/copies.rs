//! Drawing a shared condition more than once (step 0 of the pipeline).
//!
//! A condition with no requirements of its own is a fact that many pieces of
//! work can check. Drawn once, it drags edges across rows and ties unrelated
//! trees into one, so instead it is drawn once per **(tree, row)** among its
//! dependents: dependents that sit side by side in one tree share a copy
//! right under them, and every other group gets a copy of its own.
//!
//! Trees here are the weakly connected components found *without* the edges
//! into such conditions — otherwise the shared condition would join them
//! all and the rule would never split anything. Rows are the dependents'
//! ranks, which those conditions cannot change (they are sinks).
//!
//! Only the drawing is split. Each copy is a node of its own in the graph
//! handed to the rest of the pipeline, and [`Copies`] maps it back. A node's
//! first copy — the one serving its oldest dependent — keeps the node's own
//! id, so a node that is not duplicated is drawn under its own id, and the
//! first copy stays put as dependents come and go.

use std::collections::{BTreeMap, HashMap, HashSet};

use base::{Edge, EdgeId, Graph, Node, NodeId, NodeKind};

use crate::{cycle, rank, tree::Forest};

/// Which drawn nodes are copies of which graph nodes, and which edges run to
/// a copy rather than to the node itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Copies {
  /// Every extra copy, to the node it draws.
  node_of: HashMap<NodeId, NodeId>,
  /// How many times each duplicated node is drawn.
  counts:  HashMap<NodeId, usize>,
  /// Every edge redirected to an extra copy, to that copy.
  ends:    HashMap<EdgeId, NodeId>,
}

impl Copies {
  /// The graph node `drawn` draws: itself, unless it is an extra copy.
  pub fn node(&self, drawn: NodeId) -> NodeId {
    self.node_of.get(&drawn).copied().unwrap_or(drawn)
  }

  /// How many times `node` is drawn: 1 unless it is duplicated.
  pub fn count(&self, node: NodeId) -> usize {
    self.counts.get(&node).copied().unwrap_or(1)
  }

  /// The drawn node `edge` runs to: its requirement, or the copy of it that
  /// serves the edge's dependent.
  pub fn end(&self, edge: &Edge) -> NodeId {
    self.ends.get(&edge.id).copied().unwrap_or(edge.to)
  }
}

/// Split every shared leaf condition of `graph` into its copies. Returns the
/// graph to lay out, in which each copy is a node and each edge runs to the
/// copy that serves it, and the map back.
pub(crate) fn split(graph: &Graph) -> (Graph, Copies) {
  let leaves: HashSet<NodeId> = graph
    .nodes()
    .filter(|n| matches!(n.kind, NodeKind::Condition { .. }))
    .filter(|n| graph.requirements_of(n.id).next().is_none())
    .map(|n| n.id)
    .collect();
  if leaves.is_empty() {
    return (graph.clone(), Copies::default());
  }

  let layering = rank::layer(graph, &cycle::feedback_arcs(graph));
  // A leaf is never in a cycle, so its edges are never reversed: `lower` is
  // always the requirement.
  let without: Vec<_> = layering
    .dag_edges
    .iter()
    .copied()
    .filter(|(_, _, lower)| !leaves.contains(lower))
    .collect();
  let tree = Forest::label(graph, &without);

  let mut drawn = graph.clone();
  let mut copies = Copies::default();
  let mut sorted: Vec<NodeId> = leaves.into_iter().collect();
  sorted.sort_unstable();
  for leaf in sorted {
    // Dependents grouped by (tree, row), each group's edges oldest
    // dependent first.
    let mut groups: BTreeMap<(usize, usize), Vec<&Edge>> = BTreeMap::new();
    for edge in graph.dependents_of(leaf) {
      let key = (tree[&edge.from], layering.ranks[&edge.from]);
      groups.entry(key).or_default().push(edge);
    }
    if groups.len() < 2 {
      continue;
    }
    let mut groups: Vec<Vec<&Edge>> = groups.into_values().collect();
    for group in &mut groups {
      group.sort_by_key(|e| e.from);
    }
    groups.sort_by_key(|g| g[0].from);

    let node = graph.node(leaf).expect("leaves come from the graph");
    copies.counts.insert(leaf, groups.len());
    // The first group keeps the node itself; the rest get copies.
    for group in &groups[1..] {
      let copy = copy_id(leaf, group[0].from);
      drawn.insert_node(Node {
        id: copy,
        ..node.clone()
      });
      copies.node_of.insert(copy, leaf);
      for edge in group {
        drawn.remove_edge(edge.id);
        drawn.insert_edge(Edge { to: copy, ..**edge });
        copies.ends.insert(edge.id, copy);
      }
    }
  }
  (drawn, copies)
}

/// The id of `node`'s copy for the group led by `dependent`: derived from
/// both, so it stays the same from one layout to the next.
fn copy_id(node: NodeId, dependent: NodeId) -> NodeId {
  // Mix the two ids through a 128-bit multiply-xorshift, so a copy's id
  // shares no structure with either (a plain xor could collide with a real
  // id built the same way).
  let mut x = node.to_u128() ^ dependent.to_u128().rotate_left(64);
  x ^= x >> 67;
  x = x.wrapping_mul(0x9e37_79b9_7f4a_7c15_f39c_c060_5ced_c835);
  x ^= x >> 61;
  NodeId::from_u128(x)
}
