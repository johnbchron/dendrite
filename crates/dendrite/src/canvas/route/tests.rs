use std::collections::HashMap;

use base::{EdgeId, NodeId};
use layout::Channel;
use masonry::kurbo::{PathEl, Point, Rect, Vec2};

use super::*;

fn edge(from: u128, to: u128) -> RenderEdge {
  RenderEdge {
    id: EdgeId::from_u128(from * 100 + to),
    from: NodeId::from_u128(from),
    to: NodeId::from_u128(to),
    reversed: false,
    to_copy: false,
  }
}

fn boxes(list: &[(u128, Rect)]) -> HashMap<NodeId, Rect> {
  list
    .iter()
    .map(|(id, r)| (NodeId::from_u128(*id), *r))
    .collect()
}

/// Between rows, an edge leaves the bottom of the upper node and enters
/// the top of the lower one — whichever way the arrow points, so a
/// reversed cycle edge climbs back up rather than cutting through boxes.
#[test]
fn edges_attach_to_the_facing_sides() {
  let upper = Rect::new(0.0, 0.0, 150.0, 50.0);
  let lower = Rect::new(0.0, 100.0, 150.0, 150.0);
  let rects = boxes(&[(1, upper), (2, lower)]);

  let down = Route::for_edges(&[edge(1, 2)], &rects, &HashMap::new())
    .remove(0)
    .unwrap();
  assert_eq!(down.start, Point::new(75.0, 50.0));
  assert_eq!(down.end, Point::new(75.0, 100.0));
  assert_eq!(down.axis, Vec2::new(0.0, 1.0));

  let up = Route::for_edges(&[edge(2, 1)], &rects, &HashMap::new())
    .remove(0)
    .unwrap();
  assert_eq!(up.start, Point::new(75.0, 100.0));
  assert_eq!(up.end, Point::new(75.0, 50.0));
  assert_eq!(up.axis, Vec2::new(0.0, -1.0));
}

/// Edges sharing a side fan out in the order of their far ends, so they
/// neither converge on one point nor cross at the node.
#[test]
fn shared_sides_spread_endpoints_in_order() {
  let parent = Rect::new(100.0, 0.0, 250.0, 50.0);
  let left = Rect::new(0.0, 100.0, 150.0, 150.0);
  let right = Rect::new(200.0, 100.0, 350.0, 150.0);
  let rects = boxes(&[(1, parent), (2, left), (3, right)]);
  // Listed right-first, to show order comes from geometry.
  let routes =
    Route::for_edges(&[edge(1, 3), edge(1, 2)], &rects, &HashMap::new());
  let (to_right, to_left) =
    (routes[0].clone().unwrap(), routes[1].clone().unwrap());
  assert!(to_left.start.x < to_right.start.x);
  assert_eq!(to_right.start.x - to_left.start.x, PORT_PITCH);
  // Centred on the side as a group.
  assert_eq!((to_left.start.x + to_right.start.x) / 2.0, 175.0);
}

/// However many edges share a side, they stay within its middle span.
#[test]
fn ports_never_leave_the_side() {
  let r = Rect::new(0.0, 0.0, 150.0, 50.0);
  for n in 1..30 {
    for k in 0..n {
      let p = Side::Top.port(r, k, n);
      assert!(p.x >= 75.0 - 75.0 * PORT_SPAN - 1e-9);
      assert!(p.x <= 75.0 + 75.0 * PORT_SPAN + 1e-9);
      assert_eq!(p.y, 0.0);
    }
  }
}

/// The arrowhead sits at the dependent's end, pointing into it; the curve
/// leaves square from the head's base and arrives square at the
/// requirement.
#[test]
fn curve_meets_the_arrowhead_square_on() {
  let route = Route {
    start: Point::new(0.0, 0.0),
    end: Point::new(80.0, 100.0),
    axis: Vec2::new(0.0, 1.0),
    via: Vec::new(),
  };
  let (curve, tip) = route.curve();
  assert_eq!(tip, Point::new(0.0, TIP_GAP));
  let els = curve.elements();
  let PathEl::MoveTo(base) = els[0] else {
    panic!("expected a move, got {els:?}");
  };
  assert_eq!(base, Point::new(0.0, tip.y + HEAD_LEN));
  let PathEl::CurveTo(c1, c2, end) = els[1] else {
    panic!("expected a cubic, got {els:?}");
  };
  assert_eq!(c1.x, base.x, "leaves straight down");
  assert_eq!(c2.x, end.x, "arrives straight down");
  assert_eq!(end, route.end);
  // The head points up, into the dependent.
  use masonry::kurbo::Shape as _;
  let head = route.head(tip).bounding_box();
  assert_eq!((head.y0, head.y1), (tip.y, base.y));
}

/// An edge that skips rows runs straight down each channel the layout
/// gave it, and a reversed one climbs them in the opposite order.
#[test]
fn skipping_edges_run_through_their_channels() {
  let top = Rect::new(0.0, 0.0, 150.0, 50.0);
  let bottom = Rect::new(0.0, 300.0, 150.0, 350.0);
  let rects = boxes(&[(1, top), (2, bottom)]);
  let channels = |id| {
    HashMap::from([(
      id,
      vec![
        Channel {
          x: 200.0,
          top: 100.0,
          bottom: 150.0,
        },
        Channel {
          x: 210.0,
          top: 200.0,
          bottom: 250.0,
        },
      ],
    )])
  };

  let down = edge(1, 2);
  let route =
    Route::for_edges(std::slice::from_ref(&down), &rects, &channels(down.id))
      .remove(0)
      .unwrap();
  assert_eq!(
    route.via,
    vec![
      (Point::new(200.0, 100.0), Point::new(200.0, 150.0)),
      (Point::new(210.0, 200.0), Point::new(210.0, 250.0)),
    ]
  );
  // The straight runs are in the drawn path.
  let (curve, _) = route.curve();
  let lines: Vec<Point> = curve
    .elements()
    .iter()
    .filter_map(|el| match el {
      PathEl::LineTo(p) => Some(*p),
      _ => None,
    })
    .collect();
  assert_eq!(
    lines,
    vec![Point::new(200.0, 150.0), Point::new(210.0, 250.0)]
  );

  let up = edge(2, 1);
  let route =
    Route::for_edges(std::slice::from_ref(&up), &rects, &channels(up.id))
      .remove(0)
      .unwrap();
  assert_eq!(
    route.via,
    vec![
      (Point::new(210.0, 250.0), Point::new(210.0, 200.0)),
      (Point::new(200.0, 150.0), Point::new(200.0, 100.0)),
    ]
  );
}

/// An edge is kept while any of it (curve, channel, arrowhead) could
/// show, including a channel far from both ends.
#[test]
fn route_bounds_cover_the_whole_drawn_edge() {
  let route = Route {
    start: Point::new(0.0, 0.0),
    end: Point::new(0.0, 400.0),
    axis: Vec2::new(0.0, 1.0),
    via: vec![(Point::new(300.0, 100.0), Point::new(300.0, 300.0))],
  };
  let b = route.bounds();
  let (curve, tip) = route.curve();
  use masonry::kurbo::Shape as _;
  let drawn = curve.bounding_box().union_pt(tip);
  assert!(
    b.contains(drawn.origin()) && b.contains(Point::new(drawn.x1, drawn.y1))
  );
  // A view that only sees the detour still keeps the edge.
  assert!(b.overlaps(Rect::new(290.0, 150.0, 310.0, 160.0)));
}
