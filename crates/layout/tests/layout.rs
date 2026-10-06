//! Tests for the Sugiyama layout pipeline (PLAN §4, Milestone 4).

use base::{Edge, EdgeId, EdgeKind, Graph, NodeId, NodeKind};
use layout::{Layout, LayoutConfig, Pos, Size, Slot};

fn nid(n: u128) -> NodeId {
  NodeId::from_u128(n)
}

/// Every drawn node's centre, with every box at the default size. The
/// layout carries no coordinates of its own; the caller places it once it
/// knows the sizes, and these tests have none to measure.
fn placed(l: &Layout) -> std::collections::HashMap<NodeId, Pos> {
  let cfg = LayoutConfig::default();
  l.arrangement.place(&cfg, |_| cfg.node_size).nodes
}

/// Where `node`'s box lands, at the default size.
fn pos(l: &Layout, node: NodeId) -> Option<Pos> {
  placed(l).get(&node).copied()
}

/// `node`'s rank: the row of the arrangement it sits in, 0 at the top.
fn rank(l: &Layout, node: NodeId) -> Option<usize> {
  l.arrangement
    .rows
    .iter()
    .position(|row| row.contains(&Slot::Node(node)))
}

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
fn dependent_ranks_above_requirement() {
  // 0 -> 1 -> 2: "0 requires 1 requires 2". Goal 0 at top (rank 0),
  // requirements strictly below.
  let g = build(3, &[(10, 0, 1), (11, 1, 2)]);
  let l = Layout::compute(&g, &LayoutConfig::default());
  assert_eq!(rank(&l, nid(0)), Some(0), "goal at the top");
  assert!(rank(&l, nid(0)).unwrap() < rank(&l, nid(1)).unwrap());
  assert!(rank(&l, nid(1)).unwrap() < rank(&l, nid(2)).unwrap());
  // y increases downward with rank.
  assert!(pos(&l, nid(0)).unwrap().y < pos(&l, nid(2)).unwrap().y);
}

#[test]
fn order_hint_seeds_within_level_order() {
  // Two independent sibling roots with distinct order hints, no edges, so no
  // crossing pressure. The higher hint must sit to the right.
  let mut g = Graph::new();
  g.insert_node(base::Node::new(nid(1), "a", NodeKind::task(), 5.0));
  g.insert_node(base::Node::new(nid(2), "b", NodeKind::task(), 1.0));
  let l = Layout::compute(&g, &LayoutConfig::default());
  // Same rank (both roots), ordered by hint: node 2 (hint 1) left of node 1.
  assert_eq!(rank(&l, nid(1)), rank(&l, nid(2)));
  assert!(
    pos(&l, nid(2)).unwrap().x < pos(&l, nid(1)).unwrap().x,
    "lower order_hint sits further left"
  );
}

#[test]
fn ranks_are_centred_on_a_shared_axis() {
  // Rank 0: node 0 alone. Rank 1: nodes 1, 2, 3 (odd count).
  let g = build(4, &[(10, 0, 1), (11, 0, 2), (12, 0, 3)]);
  let cfg = LayoutConfig::default();
  let l = Layout::compute(&g, &cfg);

  // Odd count: the middle node sits on the axis, its neighbours one spacing
  // either side.
  let mut row: Vec<f64> = [1, 2, 3]
    .iter()
    .map(|i| pos(&l, nid(*i)).unwrap().x)
    .collect();
  row.sort_by(f64::total_cmp);
  let pitch = cfg.node_size.w + cfg.x_gap;
  assert_eq!(row, vec![-pitch, 0.0, pitch]);

  // A single-node rank is centred too, so the goal sits above the middle
  // requirement rather than above the leftmost one.
  assert_eq!(pos(&l, nid(0)).unwrap().x, 0.0);
}

#[test]
fn even_count_rank_straddles_the_axis() {
  // Rank 1 holds nodes 1 and 2: no node on the axis, but the row's midpoint
  // is on it.
  let g = build(3, &[(10, 0, 1), (11, 0, 2)]);
  let cfg = LayoutConfig::default();
  let l = Layout::compute(&g, &cfg);
  let (x1, x2) = (pos(&l, nid(1)).unwrap().x, pos(&l, nid(2)).unwrap().x);
  assert_eq!(x1 + x2, 0.0, "row straddles the axis");
  assert_eq!((x2 - x1).abs(), cfg.node_size.w + cfg.x_gap);
}

#[test]
fn placement_follows_node_sizes() {
  // Rank 0: node 0. Rank 1: nodes 1 (wide) and 2 (tall). Rank 2: node 3.
  let g = build(4, &[(10, 0, 1), (11, 0, 2), (12, 1, 3)]);
  let cfg = LayoutConfig::default();
  let l = Layout::compute(&g, &cfg);
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
  let l = Layout::compute(&g, &cfg);
  let tree_a = [0, 2, 4];
  let tree_b = [1, 3, 5];
  let xs = |ids: &[u128]| -> Vec<f64> {
    ids.iter().map(|i| pos(&l, nid(*i)).unwrap().x).collect()
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
  assert_eq!(pos(&l, nid(0)).unwrap().y, pos(&l, nid(1)).unwrap().y);
}

#[test]
fn a_lone_tree_is_centred_in_its_own_column() {
  // The parent sits over the middle of its two children even though an
  // unrelated singleton shares its row.
  let g = build(4, &[(10, 0, 1), (11, 0, 2)]);
  let l = Layout::compute(&g, &LayoutConfig::default());
  let (x0, x1, x2) = (
    pos(&l, nid(0)).unwrap().x,
    pos(&l, nid(1)).unwrap().x,
    pos(&l, nid(2)).unwrap().x,
  );
  assert!((x0 - (x1 + x2) / 2.0).abs() < 1e-9);
}

#[test]
fn a_node_sits_over_its_median_requirement() {
  // 0 requires 1 and 2; 1 requires 3, 4 and 5, and 5 requires 6.
  // Centring each row on its own left 1 off the middle of 3..5; lined up,
  // it sits right over the middle one, and 6 right under 5.
  let g = build(
    7,
    &[
      (10, 0, 1),
      (11, 0, 2),
      (12, 1, 3),
      (13, 1, 4),
      (14, 1, 5),
      (15, 5, 6),
    ],
  );
  let l = Layout::compute(&g, &LayoutConfig::default());
  let x = |i| pos(&l, nid(i)).unwrap().x;
  let mut below = [x(3), x(4), x(5)];
  below.sort_by(f64::total_cmp);
  assert!(
    (x(1) - below[1]).abs() < 1e-9,
    "1 over its middle requirement"
  );
  assert!((x(5) - x(6)).abs() < 1e-9, "6 under 5");
  // The goal sits between its two requirements.
  assert!(x(1) < x(0) && x(0) < x(2));
}

#[test]
fn long_edges_pass_through_their_own_gap() {
  // 0 -> 1 -> 2 -> 3 is a chain, and 0 -> 3 skips ranks 1 and 2. The skip
  // must not be drawn through nodes 1 and 2.
  let g = build(4, &[(10, 0, 1), (11, 1, 2), (12, 2, 3), (13, 0, 3)]);
  let cfg = LayoutConfig::default();
  let l = Layout::compute(&g, &cfg);
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
  // Channels run top row first, and straight down.
  assert!(skip[0].bottom < skip[1].top);
  assert_eq!(skip[0].x, skip[1].x, "a long edge runs straight");

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
  let l = Layout::compute(&g, &cfg);
  let reversed: Vec<_> = l.reversed_edges.iter().copied().collect();
  assert_eq!(reversed.len(), 1);
  let placed = l.arrangement.place(&cfg, |_| cfg.node_size);
  assert_eq!(placed.channels[&reversed[0]].len(), 2);
}

#[test]
fn self_loop_is_reversed_and_does_not_hang() {
  let g = build(1, &[(10, 0, 0)]);
  let l = Layout::compute(&g, &LayoutConfig::default());
  assert_eq!(rank(&l, nid(0)), Some(0));
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
      let l = Layout::compute(&g, &LayoutConfig::default());
      prop_assert_eq!(placed(&l).len() as u128, n);
    }

    /// Non-reversed edges always point strictly downward in rank.
    #[test]
    fn acyclic_edges_go_down((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let l = Layout::compute(&g, &LayoutConfig::default());
      for e in g.edges() {
        if e.from == e.to || l.is_reversed(e.id) {
          continue;
        }
        prop_assert!(rank(&l, e.from).unwrap() < rank(&l, e.to).unwrap());
      }
    }

    /// Every edge that skips ranks gets one channel per skipped rank, and no
    /// channel runs through a node's box.
    #[test]
    fn channels_never_cross_nodes((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let cfg = LayoutConfig::default();
      let l = Layout::compute(&g, &cfg);
      let placed = l.arrangement.place(&cfg, |_| cfg.node_size);
      let (hw, hh) = (cfg.node_size.w / 2.0, cfg.node_size.h / 2.0);
      for e in g.edges() {
        let span = rank(&l, e.from).unwrap().abs_diff(rank(&l, e.to).unwrap());
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

    /// However the nodes line up, every row keeps its order with at least
    /// a gap between neighbouring boxes, whatever their widths.
    #[test]
    fn rows_keep_their_order_and_gaps((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let cfg = LayoutConfig::default();
      let l = Layout::compute(&g, &cfg);
      let width = |id: NodeId| {
        let i = (0..n).find(|i| nid(*i) == id).unwrap();
        60.0 + 37.0 * (i % 5) as f64
      };
      let size = |id| Size { w: width(id), h: 40.0 };
      let placed = l.arrangement.place(&cfg, size);
      for row in &l.arrangement.rows {
        let nodes: Vec<NodeId> = row
          .iter()
          .filter_map(|s| match s {
            Slot::Node(n) => Some(*n),
            Slot::Bend { .. } => None,
          })
          .collect();
        for pair in nodes.windows(2) {
          let (a, b) = (pair[0], pair[1]);
          // Trees take columns of their own, in order of first appearance.
          let tree = |n| l.arrangement.tree[&Slot::Node(n)];
          if tree(a) != tree(b) {
            continue;
          }
          let need = (width(a) + width(b)) / 2.0 + cfg.x_gap;
          let got = placed.nodes[&b].x - placed.nodes[&a].x;
          prop_assert!(got >= need - 1e-9, "{got} < {need}");
        }
      }
    }

    /// Layout is a pure function of its input.
    #[test]
    fn deterministic((n, edges) in arb_graph()) {
      let g = build_props(n, &edges);
      let cfg = LayoutConfig::default();
      prop_assert_eq!(
        Layout::compute(&g, &cfg).arrangement,
        Layout::compute(&g, &cfg).arrangement
      );
    }
  }
}

mod copies {
  use super::*;

  /// [`build`], with the nodes in `conditions` made conditions.
  fn with_conditions(
    n: u128,
    edges: &[(u128, u128, u128)],
    conditions: &[u128],
  ) -> Graph {
    let mut g = build(n, edges);
    for &c in conditions {
      let node = g.node(nid(c)).unwrap().clone();
      g.insert_node(base::Node {
        kind: NodeKind::condition(),
        ..node
      });
    }
    g
  }

  fn layout(g: &Graph) -> Layout {
    Layout::compute(g, &LayoutConfig::default())
  }

  /// The drawn nodes that draw `node`.
  fn drawn(l: &Layout, node: u128) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = placed(l)
      .keys()
      .copied()
      .filter(|d| l.copies.node(*d) == nid(node))
      .collect();
    out.sort_unstable();
    out
  }

  #[test]
  fn row_siblings_in_one_tree_share_a_condition() {
    // 0 requires 1 and 2; both require the condition 3.
    let g = with_conditions(
      4,
      &[(10, 0, 1), (11, 0, 2), (12, 1, 3), (13, 2, 3)],
      &[3],
    );
    let l = layout(&g);
    assert_eq!(l.copies.count(nid(3)), 1);
    assert_eq!(drawn(&l, 3), [nid(3)]);
  }

  #[test]
  fn different_rows_of_one_tree_get_a_copy_each_under_them() {
    // 0 requires 1 and the condition 2; 1 requires 2 as well.
    let g = with_conditions(3, &[(10, 0, 1), (11, 0, 2), (12, 1, 2)], &[2]);
    let l = layout(&g);
    assert_eq!(l.copies.count(nid(2)), 2);
    let copies = drawn(&l, 2);
    assert_eq!(copies.len(), 2);
    assert!(
      copies.contains(&nid(2)),
      "the first copy keeps the node's id"
    );

    // The node itself serves the oldest dependent, 0; each copy sits one
    // row under its dependent, so no edge skips a row.
    let edge = |e: u128| *g.edge(EdgeId::from_u128(e)).unwrap();
    assert_eq!(l.copies.end(&edge(11)), nid(2));
    let other = l.copies.end(&edge(12));
    assert_ne!(other, nid(2));
    assert_eq!(rank(&l, nid(2)), Some(1));
    assert_eq!(rank(&l, other), Some(2));
    assert!(
      l.arrangement
        .rows
        .iter()
        .flatten()
        .all(|s| matches!(s, Slot::Node(_))),
      "no bends"
    );
  }

  #[test]
  fn separate_trees_get_a_copy_each_and_stay_apart() {
    // 0 and 1 are unrelated but for the condition 2.
    let g = with_conditions(3, &[(10, 0, 2), (11, 1, 2)], &[2]);
    let l = layout(&g);
    assert_eq!(l.copies.count(nid(2)), 2);
    let tree = |n: NodeId| l.arrangement.tree[&Slot::Node(n)];
    assert_ne!(tree(nid(0)), tree(nid(1)));

    // The same graph lays out the same copies.
    assert_eq!(drawn(&l, 2), drawn(&layout(&g), 2));
  }

  #[test]
  fn tasks_and_conditions_with_requirements_are_drawn_once() {
    let task = build(3, &[(10, 0, 2), (11, 1, 2)]);
    assert_eq!(layout(&task).copies.count(nid(2)), 1);
    assert_eq!(placed(&layout(&task)).len(), 3);

    let gated = with_conditions(4, &[(10, 0, 2), (11, 1, 2), (12, 2, 3)], &[2]);
    assert_eq!(layout(&gated).copies.count(nid(2)), 1);
    assert_eq!(placed(&layout(&gated)).len(), 4);
  }
}
