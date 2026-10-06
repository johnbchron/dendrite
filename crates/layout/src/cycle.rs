//! Feedback-arc cycle cut (PLAN §4 step 1, risk §6.2).
//!
//! Sugiyama layering needs a DAG, but the global graph may contain cycles
//! (which are allowed but flagged, PLAN §2). We break them with a greedy
//! depth-first heuristic: any edge whose target is still on the DFS stack is
//! a back edge and gets marked *reversed*. The reversed edges are left in
//! place logically — the ranking stage simply treats them as flowing the
//! other way — and the UI styles them as the backward edges of a cycle.
//!
//! The heuristic is not optimal (minimum feedback arc set is NP-hard), which
//! PLAN §6.2 accepts. It is fully deterministic: nodes and adjacency are
//! visited in sorted-id order.

use std::collections::{HashMap, HashSet};

use base::{EdgeId, Graph, NodeId};

/// Colours for the iterative DFS.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
  /// Not yet visited.
  White,
  /// On the current DFS stack (an ancestor of the node being explored).
  Gray,
  /// Fully explored.
  Black,
}

/// Compute the set of edges to reverse so the remaining graph is acyclic.
///
/// Deterministic for a given graph regardless of internal map ordering.
pub fn feedback_arcs(graph: &Graph) -> HashSet<EdgeId> {
  // Deterministic adjacency: for each node, its outgoing requirement edges
  // as (target, edge id), sorted so exploration order is stable.
  let mut adj: HashMap<NodeId, Vec<(NodeId, EdgeId)>> = HashMap::new();
  let mut nodes: Vec<NodeId> = graph.nodes().map(|n| n.id).collect();
  nodes.sort_unstable();
  for &n in &nodes {
    let mut succ: Vec<(NodeId, EdgeId)> =
      graph.requirements_of(n).map(|e| (e.to, e.id)).collect();
    succ.sort_unstable();
    adj.insert(n, succ);
  }

  let mut color: HashMap<NodeId, Color> =
    nodes.iter().map(|&n| (n, Color::White)).collect();
  let mut reversed: HashSet<EdgeId> = HashSet::new();

  // Explicit-stack DFS. Each frame tracks how far through its successor list
  // it has progressed, so recursion never touches the call stack.
  struct Frame {
    node: NodeId,
    cursor: usize,
  }

  for &start in &nodes {
    if color[&start] != Color::White {
      continue;
    }
    color.insert(start, Color::Gray);
    let mut stack = vec![Frame {
      node: start,
      cursor: 0,
    }];

    while let Some(frame) = stack.last_mut() {
      let node = frame.node;
      let succ = &adj[&node];
      if frame.cursor < succ.len() {
        let (to, edge) = succ[frame.cursor];
        frame.cursor += 1;
        match color[&to] {
          Color::White => {
            color.insert(to, Color::Gray);
            stack.push(Frame {
              node: to,
              cursor: 0,
            });
          }
          // Target is an ancestor still on the stack → back edge.
          Color::Gray => {
            reversed.insert(edge);
          }
          Color::Black => {}
        }
      } else {
        color.insert(node, Color::Black);
        stack.pop();
      }
    }
  }

  reversed
}
