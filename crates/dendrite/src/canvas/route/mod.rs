//! Edge routing: where each edge leaves and enters its nodes, the channels
//! it runs through, and the curve drawn along them.

use std::collections::HashMap;

use base::{EdgeId, NodeId};
use layout::Channel;
use masonry::kurbo::{BezPath, Point, Rect, Triangle, Vec2};

use super::RenderEdge;

/// Arrowhead length and half-width, in world units.
pub(super) const HEAD_LEN: f64 = 9.0;
const HEAD_HALF_W: f64 = 4.5;
/// Space left between an arrowhead's tip and the node it points at.
pub(super) const TIP_GAP: f64 = 2.0;
/// Spacing between edge endpoints that share one side of a node.
const PORT_PITCH: f64 = 16.0;
/// Fraction of a side's length that endpoints may spread across.
const PORT_SPAN: f64 = 0.7;
/// Shortest handle on an edge's curves, so a hop between close rows still
/// bends rather than kinking.
const MIN_REACH: f64 = 24.0;

/// Where an edge leaves its dependent and enters its requirement, and which
/// way it travels there.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Route {
  /// On the border of the `from` node, where the arrowhead's tip goes.
  pub(super) start: Point,
  /// On the border of the `to` node.
  pub(super) end: Point,
  /// Unit direction the edge leaves `start` and arrives at `end` along:
  /// down or up between rows, sideways within one.
  pub(super) axis: Vec2,
  /// For an edge that skips rows, the straight run it makes through each
  /// skipped row, as `(entry, exit)` in travel order. Following these keeps
  /// the edge in the gap the layout reserved instead of crossing nodes.
  pub(super) via: Vec<(Point, Point)>,
}

/// The side of a box an edge attaches to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
  Top,
  Bottom,
  Left,
  Right,
}

/// One edge's claim on a side of a node, before it is given a point there.
struct Port {
  /// Where the edge heads next, along the side, which orders the ports.
  along: f64,
  /// The edge's index.
  edge: usize,
  /// Whether the edge starts here (else it ends here).
  is_start: bool,
}

impl Route {
  /// Route every edge between the boxes in `rects`, through its `channels`
  /// if it skips rows, index-aligned with `edges` (`None` where an end is
  /// not placed).
  ///
  /// Edges run bottom-to-top between rows — leaving the lower side of the
  /// node above and entering the upper side of the node below, whichever
  /// way the arrow points — and side-to-side within a row. Where several
  /// edges share one side of a node, their endpoints are spread along it in
  /// the order of their far ends, so they fan out instead of converging on
  /// a single point and their curves do not cross at the node.
  pub(super) fn for_edges(
    edges: &[RenderEdge],
    rects: &HashMap<NodeId, Rect>,
    channels: &HashMap<EdgeId, Vec<Channel>>,
  ) -> Vec<Option<Route>> {
    // First pass: sides, axes and waypoints, plus who attaches where.
    let mut sides: Vec<Option<Vec2>> = Vec::new();
    let mut vias: Vec<Vec<(Point, Point)>> = Vec::new();
    let mut ports: HashMap<(NodeId, Side), Vec<Port>> = HashMap::new();
    for (i, edge) in edges.iter().enumerate() {
      let (Some(a), Some(b)) = (rects.get(&edge.from), rects.get(&edge.to))
      else {
        sides.push(None);
        vias.push(Vec::new());
        continue;
      };
      let (from_side, to_side, axis) = Side::facing(*a, *b);
      let via = Self::channels_between(channels.get(&edge.id), axis);
      // Ports along a side are ordered by where the edge heads next: its
      // first channel from the start, its last into the end, else the far
      // node.
      let along = |p: Point| if axis.x == 0.0 { p.x } else { p.y };
      let next = via.first().map_or(b.center(), |v| v.0);
      let prev = via.last().map_or(a.center(), |v| v.1);
      ports.entry((edge.from, from_side)).or_default().push(Port {
        along: along(next),
        edge: i,
        is_start: true,
      });
      ports.entry((edge.to, to_side)).or_default().push(Port {
        along: along(prev),
        edge: i,
        is_start: false,
      });
      sides.push(Some(axis));
      vias.push(via);
    }

    let mut starts: Vec<Option<Point>> = vec![None; edges.len()];
    let mut ends: Vec<Option<Point>> = vec![None; edges.len()];
    for ((node, side), mut list) in ports {
      let rect = rects[&node];
      list
        .sort_by(|x, y| x.along.total_cmp(&y.along).then(x.edge.cmp(&y.edge)));
      let n = list.len();
      for (k, port) in list.iter().enumerate() {
        let p = side.port(rect, k, n);
        if port.is_start {
          starts[port.edge] = Some(p);
        } else {
          ends[port.edge] = Some(p);
        }
      }
    }

    sides
      .iter()
      .enumerate()
      .map(|(i, axis)| {
        Some(Route {
          start: starts[i]?,
          end: ends[i]?,
          axis: (*axis)?,
          via: vias[i].clone(),
        })
      })
      .collect()
  }

  /// The straight runs an edge travelling along `axis` makes through its
  /// channels, in travel order. Channels come top row first, each entered at
  /// the top; an edge travelling up walks them in reverse and enters each at
  /// the bottom.
  fn channels_between(
    channels: Option<&Vec<Channel>>,
    axis: Vec2,
  ) -> Vec<(Point, Point)> {
    let mut via: Vec<(Point, Point)> = channels
      .into_iter()
      .flatten()
      .map(|c| (Point::new(c.x, c.top), Point::new(c.x, c.bottom)))
      .collect();
    if axis.y < 0.0 {
      via.reverse();
      for (entry, exit) in &mut via {
        std::mem::swap(entry, exit);
      }
    }
    via
  }

  /// A box containing everything drawn for this route: its endpoints and
  /// channels, grown by how far a curve's handles and the arrowhead can
  /// reach past them.
  pub(super) fn bounds(&self) -> Rect {
    let mut r = Rect::from_points(self.start, self.end);
    for &(entry, exit) in &self.via {
      r = r.union_pt(entry).union_pt(exit);
    }
    // Handles reach at most `MIN_REACH` past an endpoint along the axis;
    // sideways, the curve stays within its endpoints.
    let pad = MIN_REACH + HEAD_HALF_W;
    r.inflate(pad, pad)
  }

  /// The curve the edge is drawn along: cubics between rows that leave and
  /// arrive square to whatever they join, so edges bend smoothly the way
  /// Mermaid draws them, with a straight run down each skipped row's
  /// channel. The arrowhead points into the dependent, so work reads as
  /// flowing from a requirement to what it unblocks: the curve starts at
  /// the arrowhead's base and runs to the requirement. Returns the curve and
  /// the tip.
  pub(super) fn curve(&self) -> (BezPath, Point) {
    let axis = self.axis;
    let tip = self.start + axis * TIP_GAP;
    let base = tip + axis * HEAD_LEN;
    // Handles reach halfway along the travel axis, with a floor so a short
    // hop between close rows still bends rather than kinking.
    let bend = |path: &mut BezPath, from: Point, to: Point| {
      let reach = ((to - from).dot(axis).abs() / 2.0).max(MIN_REACH);
      path.curve_to(from + axis * reach, to - axis * reach, to);
    };
    let mut path = BezPath::new();
    path.move_to(base);
    let mut at = base;
    for &(entry, exit) in &self.via {
      bend(&mut path, at, entry);
      path.line_to(exit);
      at = exit;
    }
    bend(&mut path, at, self.end);
    (path, tip)
  }

  /// The arrowhead at `tip`, pointing back along the travel axis into the
  /// dependent: the curve leaves the node square, so the head lines up with
  /// it exactly.
  pub(super) fn head(&self, tip: Point) -> Triangle {
    let base = tip + self.axis * HEAD_LEN;
    let perp = self.axis.turn_90() * HEAD_HALF_W;
    Triangle::new(tip, base + perp, base - perp)
  }
}

impl Side {
  /// The sides of `a` and `b` an edge from `a` to `b` joins, and the unit
  /// direction it travels: down or up between rows, else sideways.
  fn facing(a: Rect, b: Rect) -> (Side, Side, Vec2) {
    if b.y0 >= a.y1 {
      (Side::Bottom, Side::Top, Vec2::new(0.0, 1.0))
    } else if b.y1 <= a.y0 {
      (Side::Top, Side::Bottom, Vec2::new(0.0, -1.0))
    } else if b.center().x >= a.center().x {
      (Side::Right, Side::Left, Vec2::new(1.0, 0.0))
    } else {
      (Side::Left, Side::Right, Vec2::new(-1.0, 0.0))
    }
  }

  /// The `k`th of `n` evenly spaced endpoints on this side of `rect`,
  /// centred on the side and never spreading past [`PORT_SPAN`] of it.
  fn port(self, rect: Rect, k: usize, n: usize) -> Point {
    let len = match self {
      Side::Top | Side::Bottom => rect.width(),
      Side::Left | Side::Right => rect.height(),
    };
    let span = (PORT_PITCH * n.saturating_sub(1) as f64).min(len * PORT_SPAN);
    let offset = if n > 1 {
      -span / 2.0 + span * k as f64 / (n - 1) as f64
    } else {
      0.0
    };
    let c = rect.center();
    match self {
      Side::Top => Point::new(c.x + offset, rect.y0),
      Side::Bottom => Point::new(c.x + offset, rect.y1),
      Side::Left => Point::new(rect.x0, c.y + offset),
      Side::Right => Point::new(rect.x1, c.y + offset),
    }
  }
}

#[cfg(test)]
mod tests;
