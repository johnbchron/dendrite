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
  states:    HashMap<NodeId, NodeState>,
  /// Nodes that are immediately executable: not done, not cyclic, all
  /// requirement targets satisfied. Both Ready tasks and Pending manual
  /// conditions whose requirements are met land here; formula conditions
  /// never do.
  ready:     HashSet<NodeId>,
  /// Nodes that count as satisfied for gating: completed tasks, satisfied
  /// manual conditions, and formula conditions whose atom holds.
  satisfied: HashSet<NodeId>,
  /// Each formula condition's evaluation.
  truths:    HashMap<NodeId, Truth>,
  /// The earliest [`Truth::until`] over all formula conditions.
  horizon:   Option<Timestamp>,
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
  pub fn is_ready(&self, node: NodeId) -> bool { self.ready.contains(&node) }

  /// All immediately-executable nodes.
  pub fn ready_nodes(&self) -> &HashSet<NodeId> { &self.ready }

  /// Whether `node` counts as satisfied for gating the work that requires
  /// it: a completed task, a satisfied manual condition, or a formula
  /// condition whose atom holds.
  pub fn is_satisfied(&self, node: NodeId) -> bool {
    self.satisfied.contains(&node)
  }

  /// A formula condition's evaluation, for the inspector's "why".
  pub fn truth(&self, node: NodeId) -> Option<&Truth> { self.truths.get(&node) }

  /// The earliest instant at which some formula condition could change by
  /// time alone: recompute then. `None` when only a declared fact can
  /// change one.
  pub fn horizon(&self) -> Option<Timestamp> { self.horizon }
}

/// Detect every node that participates in a cycle of the requirement graph.
///
/// Uses Tarjan's strongly-connected-components algorithm (iterative, so it
/// is safe at 1k+ nodes). A node is cyclic if it lives in an SCC of size > 1
/// or carries a self-loop. Edges leaving formula conditions are ignored, so
/// those are never cyclic.
pub fn cyclic_nodes(graph: &Graph) -> HashSet<NodeId> {
  let mut cyclic = HashSet::new();
  for scc in tarjan_sccs(graph) {
    if scc.len() > 1 {
      cyclic.extend(scc);
    } else {
      // A single-node SCC is only cyclic if the node points at itself.
      let n = scc[0];
      if graph.requirement_targets(n).contains(&n) {
        cyclic.insert(n);
      }
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
  let reach = |forward: bool| {
    let mut seen = HashSet::from([node]);
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
      let next: Vec<NodeId> = if forward {
        graph.requirement_targets(n).into_iter().collect()
      } else {
        graph
          .dependents_of(n)
          .map(|e| e.from)
          .filter(|m| !graph.is_formula(*m))
          .collect()
      };
      for m in next {
        if seen.insert(m) {
          stack.push(m);
        }
      }
    }
    seen
  };
  let (down, up) = (reach(true), reach(false));
  let mut peers: Vec<NodeId> = down
    .intersection(&up)
    .copied()
    .filter(|n| *n != node)
    .collect();
  peers.sort_unstable();
  peers
}

/// Iterative Tarjan SCC over the requirement graph (edges `from -> to`).
///
/// Returns the strongly-connected components. The explicit work stack keeps
/// recursion off the call stack so deep graphs do not overflow.
fn tarjan_sccs(graph: &Graph) -> Vec<Vec<NodeId>> {
  #[derive(Clone, Copy)]
  struct Meta {
    index:    u32,
    lowlink:  u32,
    on_stack: bool,
  }

  // Frame of the manual DFS: the node being explored and how far through its
  // successors we have progressed.
  struct Frame {
    node: NodeId,
    succ: Vec<NodeId>,
    next: usize,
  }

  let mut meta: HashMap<NodeId, Meta> = HashMap::new();
  let mut stack: Vec<NodeId> = Vec::new();
  let mut sccs: Vec<Vec<NodeId>> = Vec::new();
  let mut counter: u32 = 0;

  let roots: Vec<NodeId> = graph.nodes().map(|n| n.id).collect();
  for root in roots {
    if meta.contains_key(&root) {
      continue;
    }
    let mut work: Vec<Frame> = vec![Frame {
      node: root,
      succ: graph.requirement_targets(root).into_iter().collect(),
      next: 0,
    }];
    meta.insert(root, Meta {
      index:    counter,
      lowlink:  counter,
      on_stack: true,
    });
    counter += 1;
    stack.push(root);

    while let Some(frame) = work.last_mut() {
      let v = frame.node;
      if frame.next < frame.succ.len() {
        let w = frame.succ[frame.next];
        frame.next += 1;
        match meta.get(&w) {
          None => {
            // Descend into an unvisited successor.
            meta.insert(w, Meta {
              index:    counter,
              lowlink:  counter,
              on_stack: true,
            });
            counter += 1;
            stack.push(w);
            work.push(Frame {
              node: w,
              succ: graph.requirement_targets(w).into_iter().collect(),
              next: 0,
            });
          }
          Some(mw) if mw.on_stack => {
            let low = mw.index;
            let mv = meta.get_mut(&v).unwrap();
            mv.lowlink = mv.lowlink.min(low);
          }
          Some(_) => {}
        }
      } else {
        // All successors explored: close out this node.
        let mv = *meta.get(&v).unwrap();
        if mv.lowlink == mv.index {
          let mut scc = Vec::new();
          while let Some(w) = stack.pop() {
            meta.get_mut(&w).unwrap().on_stack = false;
            scc.push(w);
            if w == v {
              break;
            }
          }
          sccs.push(scc);
        }
        work.pop();
        // Propagate lowlink up to the parent frame.
        if let Some(parent) = work.last() {
          let low = mv.lowlink;
          let mp = meta.get_mut(&parent.node).unwrap();
          mp.lowlink = mp.lowlink.min(low);
        }
      }
    }
  }

  sccs
}
