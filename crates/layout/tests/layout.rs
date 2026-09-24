//! Tests for the Sugiyama layout pipeline (PLAN §4, Milestone 4).

use base::{Edge, EdgeId, EdgeKind, Graph, NodeId, NodeKind};
use layout::{LayoutConfig, Size, Slot, layout};

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
  let pitch = cfg.node_size.w + cfg.x_gap;
  assert_eq!(row, vec![-pitch, 0.0, pitch]);

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
  assert_eq!((x2 - x1).abs(), cfg.node_size.w + cfg.x_gap);
}

#[test]
fn placement_follows_node_sizes() {
  // Rank 0: node 0. Rank 1: nodes 1 (wide) and 2 (tall). Rank 2: node 3.
  let g = build(4, &[(10, 0, 1), (11, 0, 2), (12, 1, 3)]);
  let cfg = LayoutConfig::default();
  let l = layout(&g, &cfg);
  let size = |n: NodeId| match n {
    n if n == nid(1) => Size { w: 300.0, h: 40.0 },
    n if n == nid(2) => Size { w: 100.0, h: 90.0 },
    _ => Size { w: 100.0, h: 40.0 },
  };
  let pos = l.arrangement.place(&cfg, size).nodes;
  let (p1, p2) = (pos[&nid(1)], pos[&nid(2)]);

  // Neighbours in a row are exactly one gap apart, box edge to box edge,
  // and the row as a whole stays centred on the axis.
  let (left, right) = if p1.x < p2.x { (p1, p2) } else { (p2, p1) };
  let (lw, rw) = if p1.x < p2.x {
    (300.0, 100.0)
  } else {
    (100.0, 300.0)
  };
  let gap = (right.x - rw / 2.0) - (left.x + lw / 2.0);
  assert!((gap - cfg.x_gap).abs() < 1e-9, "gap was {gap}");
  let row_mid = ((left.x - lw / 2.0) + (right.x + rw / 2.0)) / 2.0;
  assert!(row_mid.abs() < 1e-9, "row centred on the axis");

  // A row is as tall as its tallest node: both share its midline, and the
  // next row starts one gap below the tall node's bottom.
  assert_eq!(p1.y, p2.y);
  let top_of_3 = pos[&nid(3)].y - 20.0;
  let bottom_of_2 = p2.y + 45.0;
  assert!((top_of_3 - bottom_of_2 - cfg.y_gap).abs() < 1e-9);
}

#[test]
fn independent_trees_do_not_interleave() {
  // Tree A: 0 -> {2, 4}. Tree B: 1 -> {3, 5}. Hints alternate between the
  // trees in every row, which interleaved them when each row was simply
  // centred on its own.
  let g = build(6, &[(10, 0, 2), (11, 0, 4), (12, 1, 3), (13, 1, 5)]);
  let cfg = LayoutConfig::default();
  let l = layout(&g, &cfg);
  let tree_a = [0, 2, 4];
  let tree_b = [1, 3, 5];
  let xs = |ids: &[u128]| -> Vec<f64> {
    ids.iter().map(|i| l.pos(nid(*i)).unwrap().x).collect()
  };
  let (a, b) = (xs(&tree_a), xs(&tree_b));
  let a_max = a.iter().copied().fold(f64::MIN, f64::max);
  let a_min = a.iter().copied().fold(f64::MAX, f64::min);
  let b_max = b.iter().copied().fold(f64::MIN, f64::max);
  let b_min = b.iter().copied().fold(f64::MAX, f64::min);
  let w = cfg.node_size.w;
  // One tree sits wholly to one side of the other, a tree gap apart.
  let gap = if a_max < b_min {
    (b_min - w / 2.0) - (a_max + w / 2.0)
  } else {
    assert!(b_max < a_min, "trees interleave: {a:?} vs {b:?}");
    (a_min - w / 2.0) - (b_max + w / 2.0)
  };
  assert!((gap - cfg.tree_gap).abs() < 1e-9, "gap was {gap}");
  // Rows still line up across trees.
  assert_eq!(l.pos(nid(0)).unwrap().y, l.pos(nid(1)).unwrap().y);
}

#[test]
fn a_lone_tree_is_centred_in_its_own_column() {
  // The parent sits over the middle of its two children even though an
  // unrelated singleton shares its row.
  let g = build(4, &[(10, 0, 1), (11, 0, 2)]);
  let l = layout(&g, &LayoutConfig::default());
  let (x0, x1, x2) = (
    l.pos(nid(0)).unwrap().x,
    l.pos(nid(1)).unwrap().x,
    l.pos(nid(2)).unwrap().x,
  );
  assert!((x0 - (x1 + x2) / 2.0).abs() < 1e-9);
}

#[test]
fn long_edges_pass_through_their_own_gap() {
  // 0 -> 1 -> 2 -> 3 is a chain, and 0 -> 3 skips ranks 1 and 2. The skip
  // must not be drawn through nodes 1 and 2.
  let g = build(4, &[(10, 0, 1), (11, 1, 2), (12, 2, 3), (13, 0, 3)]);
  let cfg = LayoutConfig::default();
  let l = layout(&g, &cfg);
  let placed = l.arrangement.place(&cfg, |_| cfg.node_size);

  let skip = &placed.channels[&EdgeId::from_u128(13)];
  assert_eq!(skip.len(), 2, "one channel per skipped rank");
  let half = cfg.node_size.w / 2.0;
  for (channel, node) in skip.iter().zip([1, 2]) {
    let p = placed.nodes[&nid(node)];
    // The channel spans that node's row…
    assert!(channel.top <= p.y - cfg.node_size.h / 2.0 + 1e-9);
    assert!(channel.bottom >= p.y + cfg.node_size.h / 2.0 - 1e-9);
    // …but clears the node by at least a gap.
    let clearance = (channel.x - p.x).abs() - half - cfg.bend_width / 2.0;
    assert!(clearance >= cfg.x_gap - 1e-9, "clearance {clearance}");
  }
  // Channels run top row first.
  assert!(skip[0].bottom < skip[1].top);

  // Edges between adjacent ranks need no channels.
  for e in [10, 11, 12] {
    assert!(!placed.channels.contains_key(&EdgeId::from_u128(e)));
  }
}

#[test]
fn a_reversed_long_edge_still_gets_channels() {
  // 0 -> 1 -> 2 -> 3, plus 3 -> 0 closing a cycle that the cut reverses.
  let g = build(4, &[(10, 0, 1), (11, 1, 2), (12, 2, 3), (13, 3, 0)]);
  let cfg = LayoutConfig::default();
  let l = layout(&g, &cfg);
  let reversed: Vec<_> = l.reversed_edges.iter().copied().collect();
  assert_eq!(reversed.len(), 1);
  let placed = l.arrangement.place(&cfg, |_| cfg.node_size);
  assert_eq!(placed.channels[&reversed[0]].len(), 2);
}

#[test]
fn retain_drops_hidden_nodes_and_empty_rows() {
  let g = build(4, &[(10, 0, 1), (11, 1, 2), (12, 0, 3)]);
  let l = layout(&g, &LayoutConfig::default());
  let only = l
    .arrangement
    .retain(|s| s != Slot::Node(nid(1)) && s != Slot::Node(nid(3)));
  assert_eq!(only.rows, vec![vec![Slot::Node(nid(0))], vec![Slot::Node(
    nid(2)
  )]]);
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

    /// Every edge that skips ranks gets one channel per skipped rank, and no
    /// channel runs through a node's box.
    #[test]
    fn channels_never_cross_nodes((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let cfg = LayoutConfig::default();
      let l = layout(&g, &cfg);
      let placed = l.arrangement.place(&cfg, |_| cfg.node_size);
      let (hw, hh) = (cfg.node_size.w / 2.0, cfg.node_size.h / 2.0);
      for e in g.edges() {
        let span = l.rank(e.from).unwrap().abs_diff(l.rank(e.to).unwrap());
        let got = placed.channels.get(&e.id).map_or(0, Vec::len);
        prop_assert_eq!(got, span.saturating_sub(1));
      }
      for channel in placed.channels.values().flatten() {
        for p in placed.nodes.values() {
          let same_row = p.y - hh < channel.bottom && p.y + hh > channel.top;
          if same_row {
            prop_assert!((channel.x - p.x).abs() > hw);
          }
        }
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
