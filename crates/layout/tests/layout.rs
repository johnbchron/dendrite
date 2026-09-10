//! Tests for the Sugiyama layout pipeline (PLAN §4, Milestone 4).

use base::{Edge, EdgeId, EdgeKind, Graph, NodeId, NodeKind};
use layout::{LayoutConfig, layout};

fn nid(n: u128) -> NodeId { NodeId::from_u128(n) }

/// Build a task graph from `n` nodes (ids 0..n, order_hint = id) and edges
/// given as `(edge_id, from, to)`.
fn build(n: u128, edges: &[(u128, u128, u128)]) -> Graph {
  let mut g = Graph::new();
  for i in 0..n {
    g.insert_node(base::Node::new(
      nid(i),
      format!("n{i}"),
      NodeKind::task(),
      i as f64,
    ));
  }
  for (e, from, to) in edges {
    g.insert_edge(Edge::new(
      EdgeId::from_u128(*e),
      EdgeKind::Dependency,
      nid(*from),
      nid(*to),
    ));
  }
  g
}

#[test]
fn every_node_gets_a_position_and_rank() {
  let g = build(4, &[(10, 0, 1), (11, 1, 2), (12, 1, 3)]);
  let l = layout(&g, &LayoutConfig::default());
  for i in 0..4 {
    assert!(l.pos(nid(i)).is_some(), "node {i} positioned");
    assert!(l.rank(nid(i)).is_some(), "node {i} ranked");
  }
}

#[test]
fn dependent_ranks_above_requirement() {
  // 0 -> 1 -> 2: "0 requires 1 requires 2". Goal 0 at top (rank 0),
  // requirements strictly below.
  let g = build(3, &[(10, 0, 1), (11, 1, 2)]);
  let l = layout(&g, &LayoutConfig::default());
  assert_eq!(l.rank(nid(0)), Some(0), "goal at the top");
  assert!(l.rank(nid(0)).unwrap() < l.rank(nid(1)).unwrap());
  assert!(l.rank(nid(1)).unwrap() < l.rank(nid(2)).unwrap());
  // y increases downward with rank.
  assert!(l.pos(nid(0)).unwrap().y < l.pos(nid(2)).unwrap().y);
}

#[test]
fn non_reversed_edges_point_downward() {
  let g = build(5, &[
    (10, 0, 1),
    (11, 0, 2),
    (12, 1, 3),
    (13, 2, 3),
    (14, 3, 4),
  ]);
  let l = layout(&g, &LayoutConfig::default());
  for e in g.edges() {
    if l.is_reversed(e.id) {
      continue;
    }
    let rf = l.rank(e.from).unwrap();
    let rt = l.rank(e.to).unwrap();
    assert!(
      rf < rt,
      "edge {:?} should point downward: {rf} < {rt}",
      e.id
    );
  }
}

#[test]
fn layout_is_deterministic() {
  let g = build(6, &[
    (10, 0, 2),
    (11, 1, 2),
    (12, 2, 3),
    (13, 2, 4),
    (14, 3, 5),
    (15, 4, 5),
  ]);
  let cfg = LayoutConfig::default();
  let a = layout(&g, &cfg);
  let b = layout(&g, &cfg);
  assert_eq!(a.positions, b.positions);
  assert_eq!(a.ranks, b.ranks);
}

#[test]
fn order_hint_seeds_within_level_order() {
  // Two independent sibling roots with distinct order hints, no edges, so no
  // crossing pressure. The higher hint must sit to the right.
  let mut g = Graph::new();
  g.insert_node(base::Node::new(nid(1), "a", NodeKind::task(), 5.0));
  g.insert_node(base::Node::new(nid(2), "b", NodeKind::task(), 1.0));
  let l = layout(&g, &LayoutConfig::default());
  // Same rank (both roots), ordered by hint: node 2 (hint 1) left of node 1.
  assert_eq!(l.rank(nid(1)), l.rank(nid(2)));
  assert!(
    l.pos(nid(2)).unwrap().x < l.pos(nid(1)).unwrap().x,
    "lower order_hint sits further left"
  );
}

#[test]
fn ranks_are_centred_on_a_shared_axis() {
  // Rank 0: node 0 alone. Rank 1: nodes 1, 2, 3 (odd count).
  let g = build(4, &[(10, 0, 1), (11, 0, 2), (12, 0, 3)]);
  let cfg = LayoutConfig::default();
  let l = layout(&g, &cfg);

  // Odd count: the middle node sits on the axis, its neighbours one spacing
  // either side.
  let mut row: Vec<f64> = [1, 2, 3]
    .iter()
    .map(|i| l.pos(nid(*i)).unwrap().x)
    .collect();
  row.sort_by(f64::total_cmp);
  assert_eq!(row, vec![-cfg.x_spacing, 0.0, cfg.x_spacing]);

  // A single-node rank is centred too, so the goal sits above the middle
  // requirement rather than above the leftmost one.
  assert_eq!(l.pos(nid(0)).unwrap().x, 0.0);
}

#[test]
fn even_count_rank_straddles_the_axis() {
  // Rank 1 holds nodes 1 and 2: no node on the axis, but the row's midpoint
  // is on it.
  let g = build(3, &[(10, 0, 1), (11, 0, 2)]);
  let cfg = LayoutConfig::default();
  let l = layout(&g, &cfg);
  let (x1, x2) = (l.pos(nid(1)).unwrap().x, l.pos(nid(2)).unwrap().x);
  assert_eq!(x1 + x2, 0.0, "row straddles the axis");
  assert_eq!((x2 - x1).abs(), cfg.x_spacing);
}

#[test]
fn cyclic_graph_still_ranks_all_nodes() {
  // 2-cycle 0 <-> 1 plus a tail. Layout must not panic, must rank every
  // node, and must reverse at least one edge.
  let g = build(3, &[(10, 0, 1), (11, 1, 0), (12, 1, 2)]);
  let l = layout(&g, &LayoutConfig::default());
  assert_eq!(l.ranks.len(), 3);
  assert!(l.positions.len() == 3);
  assert!(!l.reversed_edges.is_empty(), "a back edge was reversed");
}

#[test]
fn self_loop_is_reversed_and_does_not_hang() {
  let g = build(1, &[(10, 0, 0)]);
  let l = layout(&g, &LayoutConfig::default());
  assert_eq!(l.rank(nid(0)), Some(0));
  assert!(l.is_reversed(EdgeId::from_u128(10)));
}

mod props {
  use proptest::prelude::*;

  use super::*;

  fn arb_graph() -> impl Strategy<Value = (u128, Vec<(u128, u128)>)> {
    (1u128..10).prop_flat_map(|n| {
      let edge = (0..n, 0..n);
      (Just(n), prop::collection::vec(edge, 0..20))
    })
  }

  fn build_props(n: u128, edges: &[(u128, u128)]) -> Graph {
    let triples: Vec<(u128, u128, u128)> = edges
      .iter()
      .enumerate()
      .map(|(k, (a, b))| (k as u128 + 1000, *a, *b))
      .collect();
    build(n, &triples)
  }

  proptest! {
    /// Every node is positioned and ranked, whatever the graph.
    #[test]
    fn total_coverage((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let l = layout(&g, &LayoutConfig::default());
      prop_assert_eq!(l.positions.len() as u128, n);
      prop_assert_eq!(l.ranks.len() as u128, n);
    }

    /// Non-reversed edges always point strictly downward in rank.
    #[test]
    fn acyclic_edges_go_down((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let l = layout(&g, &LayoutConfig::default());
      for e in g.edges() {
        if e.from == e.to || l.is_reversed(e.id) {
          continue;
        }
        prop_assert!(l.rank(e.from).unwrap() < l.rank(e.to).unwrap());
      }
    }

    /// Layout is a pure function of its input.
    #[test]
    fn deterministic((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let cfg = LayoutConfig::default();
      prop_assert_eq!(layout(&g, &cfg).positions, layout(&g, &cfg).positions);
    }
  }
}
