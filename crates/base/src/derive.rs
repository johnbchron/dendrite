//! Derived state: readiness, cycle detection, and per-node status.
//!
//! Nothing here is stored — it is all recomputed from the [`Graph`] and the
//! [`Facts`] (PLAN §2 "Derived state"). Readiness has AND semantics (a node
//! is ready only when *all* its requirement targets are satisfied), and
//! cycle members are flagged Cyclic and treated as permanently blocked.
//! Formula conditions take their truth from their atom; they are sinks, and
//! never ready.

use std::collections::{HashMap, HashSet};

use jiff::Timestamp;
use petgraph::{
  algo::tarjan_scc,
  graphmap::DiGraphMap,
  visit::{Dfs, Reversed},
};

use crate::{
  formula::{Facts, Truth},
  graph::Graph,
  ids::NodeId,
  model::NodeKind,
};

/// The derived status of a single node (PLAN §2 table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeState {
  /// Task: `completed`.
  Completed,
  /// Task: not done, every requirement target satisfied, not cyclic.
  Ready,
  /// Task: not done, at least one requirement target unsatisfied.
  Blocked,
  /// Node is a member of a detected cycle — rendered invalid, treated as
  /// permanently blocked until the cycle is broken.
  Cyclic,
  /// Condition: `satisfied`, or its atom holds.
  Satisfied,
  /// Condition: not satisfied.
  Pending,
}

/// A computed snapshot of derived state for the whole graph.
#[derive(Clone, Debug, Default)]
pub struct Derived {
  /// Each node's status; [`NodeState::Cyclic`] marks the cycle members.
  states: HashMap<NodeId, NodeState>,
  /// Nodes that are immediately executable: not done, not cyclic, all
  /// requirement targets satisfied. Both Ready tasks and Pending manual
  /// conditions whose requirements are met land here; formula conditions
  /// never do.
  ready: HashSet<NodeId>,
  /// Nodes that count as satisfied for gating: completed tasks, satisfied
  /// manual conditions, and formula conditions whose atom holds.
  satisfied: HashSet<NodeId>,
  /// Each formula condition's evaluation.
  truths: HashMap<NodeId, Truth>,
  /// The earliest [`Truth::until`] over all formula conditions.
  horizon: Option<Timestamp>,
}

impl Derived {
  /// Compute derived state for `graph` under `facts`.
  pub fn compute(graph: &Graph, facts: &Facts) -> Self {
    let truths: HashMap<NodeId, Truth> = graph
      .nodes()
      .filter_map(|n| Some((n.id, n.kind.atom()?.eval(graph, facts))))
      .collect();
    let horizon = truths.values().filter_map(|t| t.until).min();
    let satisfied: HashSet<NodeId> = graph
      .nodes()
      .filter(|n| match truths.get(&n.id) {
        Some(truth) => truth.holds,
        None => n.kind.is_satisfied(),
      })
      .map(|n| n.id)
      .collect();

    let cyclic = cyclic_nodes(graph);
    let mut states = HashMap::with_capacity(graph.node_count());
    let mut ready = HashSet::new();

    for node in graph.nodes() {
      let is_satisfied = satisfied.contains(&node.id);
      if truths.contains_key(&node.id) {
        let state = if is_satisfied {
          NodeState::Satisfied
        } else {
          NodeState::Pending
        };
        states.insert(node.id, state);
        continue;
      }

      let in_cycle = cyclic.contains(&node.id);
      let all_reqs_met = graph
        .requirement_targets(node.id)
        .into_iter()
        .all(|t| satisfied.contains(&t));
      if !is_satisfied && !in_cycle && all_reqs_met {
        ready.insert(node.id);
      }

      let state = match (&node.kind, in_cycle, is_satisfied, all_reqs_met) {
        (_, true, ..) => NodeState::Cyclic,
        (NodeKind::Task { .. }, _, true, _) => NodeState::Completed,
        (NodeKind::Task { .. }, _, false, true) => NodeState::Ready,
        (NodeKind::Task { .. }, _, false, false) => NodeState::Blocked,
        (NodeKind::Condition { .. }, _, true, _) => NodeState::Satisfied,
        (NodeKind::Condition { .. }, _, false, _) => NodeState::Pending,
      };
      states.insert(node.id, state);
    }

    Self {
      states,
      ready,
      satisfied,
      truths,
      horizon,
    }
  }

  /// The derived state of `node`, if it exists.
  pub fn state(&self, node: NodeId) -> Option<NodeState> {
    self.states.get(&node).copied()
  }

  /// Whether `node` is immediately executable.
  pub fn is_ready(&self, node: NodeId) -> bool {
    self.ready.contains(&node)
  }

  /// All immediately-executable nodes.
  pub fn ready_nodes(&self) -> &HashSet<NodeId> {
    &self.ready
  }

  /// Whether `node` counts as satisfied for gating the work that requires
  /// it: a completed task, a satisfied manual condition, or a formula
  /// condition whose atom holds.
  pub fn is_satisfied(&self, node: NodeId) -> bool {
    self.satisfied.contains(&node)
  }

  /// A formula condition's evaluation, for the inspector's "why".
  pub fn truth(&self, node: NodeId) -> Option<&Truth> {
    self.truths.get(&node)
  }

  /// The earliest instant at which some formula condition could change by
  /// time alone: recompute then. `None` when only a declared fact can
  /// change one.
  pub fn horizon(&self) -> Option<Timestamp> {
    self.horizon
  }
}

/// Detect every node that participates in a cycle of the requirement graph.
///
/// Uses Tarjan's strongly-connected-components algorithm (petgraph's is
/// iterative, so it is safe at 1k+ nodes). A node is cyclic if it lives in
/// an SCC of size > 1 or carries a self-loop. Edges leaving formula
/// conditions are ignored, so those are never cyclic.
pub fn cyclic_nodes(graph: &Graph) -> HashSet<NodeId> {
  let g = requirement_graph(graph);
  let mut cyclic = HashSet::new();
  for scc in tarjan_scc(&g) {
    // A single-node SCC is only cyclic if the node points at itself.
    if scc.len() > 1 || g.contains_edge(scc[0], scc[0]) {
      cyclic.extend(scc);
    }
  }
  cyclic
}

/// The other nodes that share a cycle with `node`: its strongly connected
/// component, less itself, sorted. Empty when `node` is in no cycle (a lone
/// self-loop has no peers either).
///
/// Linear in the size of the graph, so meant for one node at a time (the
/// inspector's "in a cycle with …"), not for sweeping every node.
pub fn cycle_peers(graph: &Graph, node: NodeId) -> Vec<NodeId> {
  let g = requirement_graph(graph);
  let mut down = HashSet::new();
  let mut dfs = Dfs::new(&g, node);
  while let Some(n) = dfs.next(&g) {
    down.insert(n);
  }
  let mut peers = Vec::new();
  let mut dfs = Dfs::new(Reversed(&g), node);
  while let Some(n) = dfs.next(Reversed(&g)) {
    if n != node && down.contains(&n) {
      peers.push(n);
    }
  }
  peers.sort_unstable();
  peers
}

/// The requirement graph (edges `from -> to`), less the edges leaving
/// formula conditions, which are sinks.
fn requirement_graph(graph: &Graph) -> DiGraphMap<NodeId, ()> {
  let mut g = DiGraphMap::new();
  for node in graph.nodes() {
    g.add_node(node.id);
    for to in graph.requirement_targets(node.id) {
      g.add_edge(node.id, to, ());
    }
  }
  g
}
