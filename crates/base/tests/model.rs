//! Behavioural tests for the domain model (PLAN Milestone 1): readiness,
//! Tarjan cycle flagging, quest scope + actionable queries, and event/undo
//! round-trips.

use std::collections::{HashMap, HashSet};

use base::{
  Derived, Edge, EdgeKind, Event, Graph, NodeId, NodeKind, NodeState, Quest,
  QuestId, actionable, apply_batch, cycle_peers, cyclic_nodes, scope,
};

// --- helpers ------------------------------------------------------------

/// Deterministic node id from a small integer, so tests can talk about
/// nodes by number without depending on ULID randomness.
fn nid(n: u128) -> NodeId { NodeId::from_u128(n) }

fn qid(n: u128) -> QuestId { QuestId::from_u128(n) }

/// Build a graph from a node spec and edge list. `tasks` maps id -> done.
fn build(
  tasks: &[(u128, bool)],
  conditions: &[(u128, bool)],
  edges: &[(u128, u128, u128)],
) -> Graph {
  let mut g = Graph::new();
  for (i, done) in tasks {
    g.insert_node(base::Node::new(
      nid(*i),
      format!("t{i}"),
      NodeKind::Task { completed: *done },
      0.0,
    ));
  }
  for (i, sat) in conditions {
    g.insert_node(base::Node::new(
      nid(*i),
      format!("c{i}"),
      NodeKind::Condition {
        satisfied: *sat,
        source:    base::ConditionSource::Manual,
      },
      0.0,
    ));
  }
  for (e, from, to) in edges {
    g.insert_edge(Edge::new(
      base::EdgeId::from_u128(*e),
      EdgeKind::Dependency,
      nid(*from),
      nid(*to),
    ));
  }
  g
}

// --- readiness ----------------------------------------------------------

#[test]
fn task_with_no_requirements_is_ready() {
  let g = build(&[(1, false)], &[], &[]);
  let d = Derived::compute(&g);
  assert_eq!(d.state(nid(1)), Some(NodeState::Ready));
  assert!(d.is_ready(nid(1)));
}

#[test]
fn completed_task_is_not_ready() {
  let g = build(&[(1, true)], &[], &[]);
  let d = Derived::compute(&g);
  assert_eq!(d.state(nid(1)), Some(NodeState::Completed));
  assert!(!d.is_ready(nid(1)));
}

#[test]
fn task_is_blocked_until_all_requirements_satisfied() {
  // 1 requires 2 and 3; ready only when both are done.
  let g = build(&[(1, false), (2, false), (3, false)], &[], &[
    (10, 1, 2),
    (11, 1, 3),
  ]);
  let d = Derived::compute(&g);
  assert_eq!(d.state(nid(1)), Some(NodeState::Blocked));

  let g = build(&[(1, false), (2, true), (3, false)], &[], &[
    (10, 1, 2),
    (11, 1, 3),
  ]);
  let d = Derived::compute(&g);
  assert_eq!(
    d.state(nid(1)),
    Some(NodeState::Blocked),
    "one req still open"
  );

  let g = build(&[(1, false), (2, true), (3, true)], &[], &[
    (10, 1, 2),
    (11, 1, 3),
  ]);
  let d = Derived::compute(&g);
  assert_eq!(
    d.state(nid(1)),
    Some(NodeState::Ready),
    "all reqs satisfied"
  );
}

#[test]
fn condition_states_are_pending_or_satisfied() {
  let g = build(&[], &[(1, false), (2, true)], &[]);
  let d = Derived::compute(&g);
  assert_eq!(d.state(nid(1)), Some(NodeState::Pending));
  assert_eq!(d.state(nid(2)), Some(NodeState::Satisfied));
  // A pending manual condition with no requirements is actionable.
  assert!(d.is_ready(nid(1)));
}

// --- cycles -------------------------------------------------------------

#[test]
fn self_loop_is_cyclic() {
  let g = build(&[(1, false)], &[], &[(10, 1, 1)]);
  let d = Derived::compute(&g);
  assert_eq!(d.state(nid(1)), Some(NodeState::Cyclic));
  assert!(!d.is_ready(nid(1)), "cyclic nodes are never ready");
}

#[test]
fn cycle_peers_are_the_rest_of_the_component() {
  // 1 -> 2 -> 3 -> 1 is a cycle; 3 -> 4 hangs off it; 5 loops on itself.
  let g = build(
    &[(1, false), (2, false), (3, false), (4, false), (5, false)],
    &[],
    &[(10, 1, 2), (11, 2, 3), (12, 3, 1), (13, 3, 4), (14, 5, 5)],
  );
  assert_eq!(cycle_peers(&g, nid(1)), vec![nid(2), nid(3)]);
  assert_eq!(cycle_peers(&g, nid(3)), vec![nid(1), nid(2)]);
  assert!(cycle_peers(&g, nid(4)).is_empty(), "reached, but not in it");
  assert!(
    cycle_peers(&g, nid(5)).is_empty(),
    "a self-loop has no peers"
  );
}

// --- quest scope + actionable ------------------------------------------

#[test]
fn scope_pulls_in_requirement_closure() {
  // quest claims 1; 1 -> 2 -> 3. Scope = {1,2,3}, pulled-in = {2,3}.
  let mut g = build(&[(1, false), (2, false), (3, false)], &[], &[
    (10, 1, 2),
    (11, 2, 3),
  ]);
  let mut quest = Quest::new(qid(100), "epic");
  quest.claims.insert(nid(1));
  g.insert_quest(quest);

  let s = scope(&g, qid(100));
  assert_eq!(s.claimed, HashSet::from([nid(1)]));
  assert_eq!(s.pulled_in, HashSet::from([nid(2), nid(3)]));
  assert_eq!(s.len(), 3);
}

#[test]
fn actionable_is_the_ready_frontier_of_scope() {
  // 1 requires 2 requires 3. Only the deepest incomplete node (3) is ready.
  let mut g = build(&[(1, false), (2, false), (3, false)], &[], &[
    (10, 1, 2),
    (11, 2, 3),
  ]);
  let mut quest = Quest::new(qid(100), "epic");
  quest.claims.insert(nid(1));
  g.insert_quest(quest);

  let d = Derived::compute(&g);
  assert_eq!(actionable(&g, &d, qid(100)), vec![nid(3)]);

  // Finish 3: now 2 is the frontier.
  g.set_satisfied(nid(3), true);
  let d = Derived::compute(&g);
  assert_eq!(actionable(&g, &d, qid(100)), vec![nid(2)]);
}

#[test]
fn actionable_lists_pulled_in_ready_work() {
  // quest claims 1; 1 requires 2 (unclaimed). 2 is ready and must appear.
  let mut g = build(&[(1, false), (2, false)], &[], &[(10, 1, 2)]);
  let mut quest = Quest::new(qid(100), "epic");
  quest.claims.insert(nid(1));
  g.insert_quest(quest);
  let d = Derived::compute(&g);
  let act = actionable(&g, &d, qid(100));
  assert!(act.contains(&nid(2)), "pulled-in ready work is listed");
  assert!(!scope(&g, qid(100)).claimed.contains(&nid(2)));
}

// --- events + undo ------------------------------------------------------

#[test]
fn event_apply_then_inverse_round_trips() {
  let mut g = Graph::new();
  let batch = vec![
    Event::NodeAdded {
      node:       nid(1),
      kind:       NodeKind::task(),
      name:       "root".into(),
      order_hint: 0.0,
    },
    Event::NodeAdded {
      node:       nid(2),
      kind:       NodeKind::task(),
      name:       "dep".into(),
      order_hint: 1.0,
    },
    Event::EdgeAdded {
      edge: base::EdgeId::from_u128(10),
      kind: EdgeKind::Dependency,
      from: nid(1),
      to:   nid(2),
    },
    Event::QuestCreated {
      quest: qid(100),
      name:  "epic".into(),
    },
    Event::QuestClaimed {
      quest: qid(100),
      node:  nid(1),
    },
    Event::TaskCompleted {
      node:      nid(2),
      completed: true,
    },
  ];

  let before = g.clone();
  let inverse = apply_batch(&mut g, &batch);
  assert_ne!(g, before, "batch changed the graph");

  // Applying the inverse batch must restore the exact prior state.
  for e in &inverse {
    e.apply(&mut g);
  }
  assert_eq!(g, before, "inverse batch restored the original graph");
}

#[test]
fn renaming_a_quest_inverts_to_the_previous_name() {
  let mut g = Graph::new();
  apply_batch(&mut g, &[Event::QuestCreated {
    quest: qid(100),
    name:  "epic".into(),
  }]);

  let rename = Event::QuestRenamed {
    quest: qid(100),
    name:  "Ship v1".into(),
  };
  let inverse = apply_batch(&mut g, std::slice::from_ref(&rename));
  assert_eq!(g.quest(qid(100)).unwrap().name, "Ship v1");

  for e in &inverse {
    e.apply(&mut g);
  }
  assert_eq!(g.quest(qid(100)).unwrap().name, "epic", "undo restored it");

  // Renaming a quest that is not there is a no-op with no inverse.
  let missing = Event::QuestRenamed {
    quest: qid(999),
    name:  "ghost".into(),
  };
  let before = g.clone();
  assert!(missing.inverse(&g).is_empty());
  missing.apply(&mut g);
  assert_eq!(g, before);
}

#[test]
fn removing_a_node_and_undoing_restores_edges_and_claims() {
  let mut g = Graph::new();
  apply_batch(&mut g, &[
    Event::NodeAdded {
      node:       nid(1),
      kind:       NodeKind::task(),
      name:       "a".into(),
      order_hint: 0.0,
    },
    Event::NodeAdded {
      node:       nid(2),
      kind:       NodeKind::task(),
      name:       "b".into(),
      order_hint: 0.0,
    },
    Event::EdgeAdded {
      edge: base::EdgeId::from_u128(10),
      kind: EdgeKind::Dependency,
      from: nid(2),
      to:   nid(1),
    },
    Event::QuestCreated {
      quest: qid(100),
      name:  "e".into(),
    },
    Event::QuestClaimed {
      quest: qid(100),
      node:  nid(1),
    },
  ]);

  let before = g.clone();
  let inverse = apply_batch(&mut g, &[Event::NodeRemoved { node: nid(1) }]);

  // The removal cascaded: node, its incident edge and the claim are gone.
  assert!(g.node(nid(1)).is_none());
  assert_eq!(g.edges().count(), 0, "incident edge removed");
  assert!(!g.quest(qid(100)).unwrap().claims.contains(&nid(1)));

  for e in &inverse {
    e.apply(&mut g);
  }
  assert_eq!(g, before, "undo restored node, edge and claim");
}

#[test]
fn events_serialize_as_self_describing_json() {
  let e = Event::EdgeAdded {
    edge: base::EdgeId::from_u128(10),
    kind: EdgeKind::Dependency,
    from: nid(1),
    to:   nid(2),
  };
  let json = serde_json::to_string(&e).unwrap();
  assert!(json.contains("\"type\":\"edge_added\""), "{json}");
  let back: Event = serde_json::from_str(&json).unwrap();
  assert_eq!(e, back);
}

// --- property tests -----------------------------------------------------

mod props {
  use proptest::prelude::*;

  use super::*;

  /// A random graph: `n` nodes (ids 0..n) and a set of directed edges.
  fn arb_graph() -> impl Strategy<Value = (usize, Vec<(usize, usize)>)> {
    (1usize..12).prop_flat_map(|n| {
      let edge = (0..n, 0..n);
      (Just(n), prop::collection::vec(edge, 0..24))
    })
  }

  fn graph_from(n: usize, edges: &[(usize, usize)]) -> Graph {
    let mut g = Graph::new();
    for i in 0..n {
      g.insert_node(base::Node::new(
        nid(i as u128),
        format!("n{i}"),
        NodeKind::task(),
        0.0,
      ));
    }
    for (k, (a, b)) in edges.iter().enumerate() {
      g.insert_edge(Edge::new(
        base::EdgeId::from_u128(k as u128 + 1_000),
        EdgeKind::Dependency,
        nid(*a as u128),
        nid(*b as u128),
      ));
    }
    g
  }

  /// Independent, brute-force cycle oracle: a node is cyclic iff it lies on
  /// a directed path back to itself (length >= 1).
  fn brute_cyclic(n: usize, edges: &[(usize, usize)]) -> HashSet<NodeId> {
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for (a, b) in edges {
      adj.entry(*a).or_default().push(*b);
    }
    let mut cyclic = HashSet::new();
    for start in 0..n {
      // BFS following edges; can we return to `start`?
      let mut stack = adj.get(&start).cloned().unwrap_or_default();
      let mut seen = HashSet::new();
      while let Some(v) = stack.pop() {
        if v == start {
          cyclic.insert(nid(start as u128));
          break;
        }
        if seen.insert(v) {
          stack.extend(adj.get(&v).cloned().unwrap_or_default());
        }
      }
    }
    cyclic
  }

  proptest! {
    /// Tarjan's flagged set matches the mutual-reachability oracle exactly.
    #[test]
    fn tarjan_matches_reachability_oracle(
      (n, edges) in arb_graph()
    ) {
      let g = graph_from(n, &edges);
      let got = cyclic_nodes(&g);
      let want = brute_cyclic(n, &edges);
      prop_assert_eq!(got, want);
    }

    /// Readiness definition holds for every node: a task is Ready exactly
    /// when it is not done, not cyclic, and all requirement targets are
    /// satisfied.
    #[test]
    fn readiness_matches_definition(
      (n, edges) in arb_graph()
    ) {
      let g = graph_from(n, &edges);
      let d = Derived::compute(&g);
      let cyclic = cyclic_nodes(&g);
      for i in 0..n {
        let node = nid(i as u128);
        // In this generator every node is an incomplete task, so
        // "all reqs satisfied" reduces to "no requirement targets".
        let has_reqs = edges.iter().any(|(a, _)| *a == i);
        let expect_ready = !cyclic.contains(&node) && !has_reqs;
        prop_assert_eq!(d.is_ready(node), expect_ready);
      }
    }

  }
}
