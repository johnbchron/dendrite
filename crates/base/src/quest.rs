//! Quest-scoped queries (PLAN §2 "Quest-scoped queries").
//!
//! Readiness is always computed on the whole global graph; quests only
//! filter what is shown. Two core queries live here: the **scope view**
//! (claimed nodes plus the transitive requirement closure beneath them) and
//! the **actionable query** (the Ready frontier of that scope).

use std::collections::HashSet;

use crate::{
  derive::Derived,
  graph::Graph,
  ids::{NodeId, QuestId},
};

/// The set of nodes a quest renders, partitioned into the nodes it actually
/// claims and the nodes pulled in only because claimed work requires them.
///
/// Pulled-in nodes render visually distinct because completing them still
/// affects global state (PLAN §2, §5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestScope {
  /// Nodes the quest directly claims (and that still exist).
  pub claimed:   HashSet<NodeId>,
  /// Nodes reachable through the requirement closure but not claimed.
  pub pulled_in: HashSet<NodeId>,
}

impl QuestScope {
  /// Every node in scope, claimed or pulled-in.
  pub fn all(&self) -> impl Iterator<Item = NodeId> + '_ {
    self.claimed.iter().chain(self.pulled_in.iter()).copied()
  }

  /// Whether `node` is anywhere in scope.
  pub fn contains(&self, node: NodeId) -> bool {
    self.claimed.contains(&node) || self.pulled_in.contains(&node)
  }

  /// Total number of nodes in scope.
  pub fn len(&self) -> usize { self.claimed.len() + self.pulled_in.len() }

  /// Whether the scope is empty.
  pub fn is_empty(&self) -> bool {
    self.claimed.is_empty() && self.pulled_in.is_empty()
  }
}

/// Compute the scope view for `quest`: its claims plus every node reachable
/// by following requirement edges (`from -> to`) downward.
pub fn scope(graph: &Graph, quest: QuestId) -> QuestScope {
  let Some(q) = graph.quest(quest) else {
    return QuestScope::default();
  };

  // Seed the frontier with claims that still resolve to real nodes.
  let claimed: HashSet<NodeId> = q
    .claims
    .iter()
    .copied()
    .filter(|n| graph.node(*n).is_some())
    .collect();

  let mut visited: HashSet<NodeId> = claimed.clone();
  let mut frontier: Vec<NodeId> = claimed.iter().copied().collect();
  while let Some(n) = frontier.pop() {
    for edge in graph.requirements_of(n) {
      if visited.insert(edge.to) {
        frontier.push(edge.to);
      }
    }
  }

  let pulled_in: HashSet<NodeId> =
    visited.difference(&claimed).copied().collect();
  QuestScope { claimed, pulled_in }
}

/// The actionable frontier of `quest`: nodes in scope that are Ready
/// (not done, not cyclic, all requirements satisfied), evaluated all the way
/// down the graph. Includes pulled-in Ready work claimed by other quests or
/// none — the caller badges it with [`claiming_quests`] (PLAN §2, §7.2).
///
/// Results are sorted by id for a stable, deterministic order.
pub fn actionable(
  graph: &Graph,
  derived: &Derived,
  quest: QuestId,
) -> Vec<NodeId> {
  let scope = scope(graph, quest);
  let mut out: Vec<NodeId> =
    scope.all().filter(|n| derived.is_ready(*n)).collect();
  out.sort_unstable();
  out
}

/// Every quest that claims `node`, for the "belongs to other quests" badge
/// on multi-claimed and pulled-in nodes (PLAN §5). Sorted for determinism.
pub fn claiming_quests(graph: &Graph, node: NodeId) -> Vec<QuestId> {
  let mut quests: Vec<QuestId> = graph
    .quests()
    .filter(|q| q.claims.contains(&node))
    .map(|q| q.id)
    .collect();
  quests.sort_unstable();
  quests
}
