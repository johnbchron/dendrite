//! Completed trees: work that is finished, moved out of the way.
//!
//! A tree is a root (a node nothing requires) and everything beneath it.
//! It is completed when every task in it is completed and every manual
//! condition satisfied; formula conditions are left out of the question,
//! since the world decides them and they are shared across trees. A tree
//! of formula conditions alone is not completed: there is no work in it.
//!
//! Completed trees get a view of their own, and leave every other one. A
//! node shared with a tree that is not completed stays where it was, since
//! that tree still needs it.

use std::collections::HashSet;

use crate::{graph::Graph, ids::NodeId};

/// Which nodes the completed trees hold, and which of those only they hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Completed {
  /// Every node of every completed tree: what the completed view shows.
  pub trees:   HashSet<NodeId>,
  /// The nodes of completed trees that no other tree reaches: what every
  /// other view leaves out.
  pub retired: HashSet<NodeId>,
}

/// The completed trees of `graph`.
pub fn completed(graph: &Graph) -> Completed {
  let mut trees = HashSet::new();
  let mut live = HashSet::new();
  let roots = graph
    .nodes()
    .filter(|n| graph.dependents_of(n.id).next().is_none());
  for root in roots {
    let tree = beneath(graph, root.id);
    let mut work = tree
      .iter()
      .filter_map(|n| graph.node(*n))
      .filter(|n| n.kind.atom().is_none())
      .peekable();
    let done = work.peek().is_some() && work.all(|n| n.kind.is_satisfied());
    if done { &mut trees } else { &mut live }.extend(tree);
  }
  let retired = trees.difference(&live).copied().collect();
  Completed { trees, retired }
}

/// `root` and every node it requires, however deep.
fn beneath(graph: &Graph, root: NodeId) -> HashSet<NodeId> {
  let mut seen = HashSet::from([root]);
  let mut frontier = vec![root];
  while let Some(n) = frontier.pop() {
    for edge in graph.requirements_of(n) {
      if seen.insert(edge.to) {
        frontier.push(edge.to);
      }
    }
  }
  seen
}
