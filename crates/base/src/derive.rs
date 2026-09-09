//! Derived state: readiness, cycle detection, and per-node status.
//!
//! Nothing here is stored — it is all recomputed from the [`Graph`]
//! (PLAN §2 "Derived state"). Readiness has AND semantics (a node is ready
//! only when *all* its requirement targets are satisfied), subtask edges
//! gate exactly like dependencies, and cycle members are flagged Cyclic and
//! treated as permanently blocked.

use std::collections::{HashMap, HashSet};

use crate::{graph::Graph, ids::NodeId, model::NodeKind};

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
  /// Condition: `satisfied`.
  Satisfied,
  /// Condition: not satisfied.
  Pending,
}

impl NodeState {
  /// Whether this state counts as "done" for the node.
  pub fn is_done(self) -> bool {
    matches!(self, NodeState::Completed | NodeState::Satisfied)
  }
}

/// A computed snapshot of derived state for the whole graph.
#[derive(Clone, Debug, Default)]
pub struct Derived {
  states: HashMap<NodeId, NodeState>,
  cyclic: HashSet<NodeId>,
  /// Nodes that are immediately executable: not done, not cyclic, all
  /// requirement targets satisfied. Both Ready tasks and Pending conditions
  /// whose requirements are met land here.
  ready:  HashSet<NodeId>,
}

impl Derived {
  /// Compute derived state for `graph`.
  pub fn compute(graph: &Graph) -> Self {
    let cyclic = cyclic_nodes(graph);
    let mut states = HashMap::with_capacity(graph.node_count());
    let mut ready = HashSet::new();

    for node in graph.nodes() {
      let satisfied = node.kind.is_satisfied();
      let in_cycle = cyclic.contains(&node.id);
      let all_reqs_met = graph
        .requirement_targets(node.id)
        .into_iter()
        .all(|t| graph.is_satisfied(t));
      let is_ready = !satisfied && !in_cycle && all_reqs_met;
      if is_ready {
        ready.insert(node.id);
      }

      let state = match (&node.kind, in_cycle, satisfied, all_reqs_met) {
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
      cyclic,
      ready,
    }
  }

  /// The derived state of `node`, if it exists.
  pub fn state(&self, node: NodeId) -> Option<NodeState> {
    self.states.get(&node).copied()
  }

  /// Whether `node` is a member of a cycle.
  pub fn is_cyclic(&self, node: NodeId) -> bool { self.cyclic.contains(&node) }

  /// Whether `node` is immediately executable.
  pub fn is_ready(&self, node: NodeId) -> bool { self.ready.contains(&node) }

  /// All nodes flagged as cycle members.
  pub fn cyclic_nodes(&self) -> &HashSet<NodeId> { &self.cyclic }

  /// All immediately-executable nodes.
  pub fn ready_nodes(&self) -> &HashSet<NodeId> { &self.ready }
}

/// Detect every node that participates in a cycle of the requirement graph.
///
/// Uses Tarjan's strongly-connected-components algorithm (iterative, so it
/// is safe at 1k+ nodes). A node is cyclic if it lives in an SCC of size > 1
/// or carries a self-loop.
pub fn cyclic_nodes(graph: &Graph) -> HashSet<NodeId> {
  let mut cyclic = HashSet::new();
  for scc in tarjan_sccs(graph) {
    if scc.len() > 1 {
      cyclic.extend(scc);
    } else {
      // A single-node SCC is only cyclic if the node points at itself.
      let n = scc[0];
      if graph.requirements_of(n).any(|e| e.to == n) {
        cyclic.insert(n);
      }
    }
  }
  cyclic
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
