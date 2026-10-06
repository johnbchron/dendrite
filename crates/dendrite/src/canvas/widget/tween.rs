//! Easing boxes and edges from one placement to the next.
//!
//! An edit can move every box. Rather than jump, the canvas eases what it
//! draws from where the boxes were to where they now go, so the eye can
//! follow each one to its new place (PLAN §6.3). Boxes and edges new to the
//! placement fade in where they land; those that left it fade out where
//! they were last drawn.

use std::{
  collections::{HashMap, HashSet},
  hash::Hash,
};

use base::{EdgeId, NodeId};
use layout::Channel;
use masonry::kurbo::Rect;

/// How long a relayout takes to settle, in milliseconds.
const DURATION_MS: f64 = 200.0;
/// The longest step one animation frame may take, in milliseconds, so a
/// stall (the first frame after idling, or a hitch) cannot skip the motion.
const MAX_STEP_MS: f64 = 1000.0 / 30.0;

/// Every box and edge, and every long edge's channels, in world
/// coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Placed {
  /// Each box.
  pub(super) rects: HashMap<NodeId, Rect>,
  /// Each edge.
  pub(super) edges: HashSet<EdgeId>,
  /// The channels of each edge that skips rows, top row first.
  pub(super) channels: HashMap<EdgeId, Vec<Channel>>,
}

/// What to draw: where every box and edge is, including those on their way
/// out, and how much of each is shown.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Drawn {
  /// Where everything is.
  pub(super) placed: Placed,
  /// The opacity of every box fading in or out; any other is fully shown.
  boxes: HashMap<NodeId, f64>,
  /// The same for edges.
  edges: HashMap<EdgeId, f64>,
}

impl Drawn {
  /// `placed`, fully shown.
  pub(super) fn settled(placed: Placed) -> Self {
    Self {
      placed,
      ..Self::default()
    }
  }

  /// How much of box `id` is shown, from 0 to 1.
  pub(super) fn of_box(&self, id: NodeId) -> f64 {
    self.boxes.get(&id).copied().unwrap_or(1.0)
  }

  /// How much of edge `id` is shown, from 0 to 1.
  pub(super) fn of_edge(&self, id: EdgeId) -> f64 {
    self.edges.get(&id).copied().unwrap_or(1.0)
  }
}

/// A relayout in flight: what was drawn when it landed, and how far along.
#[derive(Clone, Debug)]
pub(super) struct Tween {
  /// What was drawn when the relayout landed.
  from: Drawn,
  /// Milliseconds since it landed.
  elapsed: f64,
}

impl Tween {
  /// A tween from `from` to `to`, or `None` when there is nothing to ease:
  /// nothing was drawn before (the first placement appears at once), or
  /// nothing moves, arrives, leaves or is part way through a fade.
  pub(super) fn between(from: Drawn, to: &Placed) -> Option<Self> {
    let old = &from.placed;
    if old.rects.is_empty() {
      return None;
    }
    let moves = to
      .rects
      .iter()
      .any(|(id, r)| old.rects.get(id).is_none_or(|f| f != r));
    let changes = moves
      || old.rects.keys().any(|id| !to.rects.contains_key(id))
      || old.edges != to.edges
      || !from.boxes.is_empty()
      || !from.edges.is_empty();
    changes.then_some(Self { from, elapsed: 0.0 })
  }

  /// Step `interval` nanoseconds on. Returns whether the tween has finished.
  pub(super) fn advance(&mut self, interval: u64) -> bool {
    self.elapsed += (interval as f64 / 1e6).min(MAX_STEP_MS);
    self.elapsed >= DURATION_MS
  }

  /// What to draw now, heading for `to`. Boxes that stay ease from where
  /// they were drawn; new ones fade in where they land; ones that left fade
  /// out where they were. An edge's channels ease along when it skips the
  /// same rows as before, and are drawn where they are going when not.
  pub(super) fn at(&self, to: &Placed) -> Drawn {
    let t = ease_out(self.elapsed / DURATION_MS);
    let from = &self.from;
    let keys = |rects: &HashMap<NodeId, Rect>| rects.keys().copied().collect();
    let boxes =
      fade(&keys(&from.placed.rects), &keys(&to.rects), &from.boxes, t);
    let edges = fade(&from.placed.edges, &to.edges, &from.edges, t);

    let rects = boxes
      .keys()
      .chain(to.rects.keys())
      .filter_map(|id| {
        let r = match (from.placed.rects.get(id), to.rects.get(id)) {
          (Some(&f), Some(&r)) => lerp_rect(f, r, t),
          (_, Some(&r)) => r,
          (Some(&f), None) => f,
          (None, None) => return None,
        };
        Some((*id, r))
      })
      .collect();
    let channels = edges
      .keys()
      .chain(to.edges.iter())
      .filter_map(|id| {
        let cs = match (from.placed.channels.get(id), to.channels.get(id)) {
          (Some(fs), Some(cs)) if fs.len() == cs.len() => fs
            .iter()
            .zip(cs)
            .map(|(f, c)| lerp_channel(*f, *c, t))
            .collect(),
          (_, Some(cs)) => cs.clone(),
          (Some(fs), None) if !to.edges.contains(id) => fs.clone(),
          _ => return None,
        };
        Some((*id, cs))
      })
      .collect();
    let edge_ids = to.edges.iter().chain(edges.keys()).copied().collect();
    Drawn {
      placed: Placed {
        rects,
        edges: edge_ids,
        channels,
      },
      boxes,
      edges,
    }
  }
}

/// Where a fade has got to `t` of the way through, for everything in `from`
/// or `to` not fully shown: each starts at its opacity in `from` (`shown`,
/// or 1 if not listed; 0 if it was not drawn at all) and heads for 1 if it
/// is in `to`, else 0. Something on its way out stays listed, at 0 by the
/// end; something already gone when the tween began is left out.
fn fade<K: Copy + Eq + Hash>(
  from: &HashSet<K>,
  to: &HashSet<K>,
  shown: &HashMap<K, f64>,
  t: f64,
) -> HashMap<K, f64> {
  from
    .union(to)
    .filter_map(|&id| {
      let a = if from.contains(&id) {
        shown.get(&id).copied().unwrap_or(1.0)
      } else {
        0.0
      };
      let b = if to.contains(&id) { 1.0 } else { 0.0 };
      let now = lerp(a, b, t);
      (a + b > 0.0 && now < 1.0).then_some((id, now))
    })
    .collect()
}

/// Ease-out cubic: fast start, gentle landing.
fn ease_out(t: f64) -> f64 {
  1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
  a + (b - a) * t
}

fn lerp_rect(a: Rect, b: Rect, t: f64) -> Rect {
  Rect::new(
    lerp(a.x0, b.x0, t),
    lerp(a.y0, b.y0, t),
    lerp(a.x1, b.x1, t),
    lerp(a.y1, b.y1, t),
  )
}

fn lerp_channel(a: Channel, b: Channel, t: f64) -> Channel {
  Channel {
    x: lerp(a.x, b.x, t),
    top: lerp(a.top, b.top, t),
    bottom: lerp(a.bottom, b.bottom, t),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn placed(rects: &[(NodeId, Rect)]) -> Placed {
    Placed {
      rects: rects.iter().copied().collect(),
      ..Placed::default()
    }
  }

  /// Run `tween` to its end in 16 ms frames.
  fn finish(tween: &mut Tween) {
    let mut frames = 0;
    while !tween.advance(16_000_000) {
      frames += 1;
      assert!(frames < 100, "never finished");
    }
  }

  /// A box that stays eases from its old place to its new one, starting
  /// where it was and landing exactly.
  #[test]
  fn boxes_ease_to_their_new_places() {
    let a = NodeId::new();
    let old = Rect::new(0.0, 0.0, 100.0, 40.0);
    let new = Rect::new(200.0, 80.0, 300.0, 120.0);
    let to = placed(&[(a, new)]);
    let mut tween =
      Tween::between(Drawn::settled(placed(&[(a, old)])), &to).unwrap();

    assert_eq!(tween.at(&to).placed.rects[&a], old);
    assert!(!tween.advance(50_000_000));
    let mid = tween.at(&to).placed.rects[&a];
    assert!(
      mid.x0 > old.x0 && mid.x0 < new.x0,
      "part of the way: {mid:?}"
    );
    assert_eq!(mid.size(), old.size());

    finish(&mut tween);
    assert_eq!(tween.at(&to), Drawn::settled(to), "lands exactly");
  }

  /// A new box fades in where it lands; a removed one fades out where it
  /// was, and is gone once the tween ends.
  #[test]
  fn boxes_fade_in_and_out() {
    let (stay, new, gone) = (NodeId::new(), NodeId::new(), NodeId::new());
    let r = |x| Rect::new(x, 0.0, x + 100.0, 40.0);
    let from = Drawn::settled(placed(&[(stay, r(0.0)), (gone, r(200.0))]));
    let to = placed(&[(stay, r(0.0)), (new, r(400.0))]);
    let mut tween = Tween::between(from, &to).unwrap();

    let start = tween.at(&to);
    assert_eq!(start.of_box(new), 0.0);
    assert_eq!(start.of_box(gone), 1.0);
    assert_eq!(start.placed.rects[&new], r(400.0), "lands, then fades in");

    tween.advance(50_000_000);
    let mid = tween.at(&to);
    assert!(mid.of_box(new) > 0.0 && mid.of_box(new) < 1.0);
    assert!(mid.of_box(gone) > 0.0 && mid.of_box(gone) < 1.0);
    assert_eq!(mid.placed.rects[&gone], r(200.0), "fades where it was");
    assert_eq!(mid.of_box(stay), 1.0);

    finish(&mut tween);
    let end = tween.at(&to);
    assert_eq!(end.of_box(new), 1.0);
    assert_eq!(end.of_box(gone), 0.0);
  }

  /// A relayout landing part way through a fade carries on from the
  /// opacity it had reached, rather than snapping.
  #[test]
  fn a_fade_carries_on_through_a_relayout() {
    let (stay, new) = (NodeId::new(), NodeId::new());
    let r = |x| Rect::new(x, 0.0, x + 100.0, 40.0);
    let to = placed(&[(stay, r(0.0)), (new, r(200.0))]);
    let mut first =
      Tween::between(Drawn::settled(placed(&[(stay, r(0.0))])), &to).unwrap();
    first.advance(50_000_000);
    let reached = first.at(&to);
    let part = reached.of_box(new);

    // The new box's label grew, so it moved a little.
    let to = placed(&[(stay, r(0.0)), (new, r(210.0))]);
    let second = Tween::between(reached, &to).unwrap();
    assert_eq!(second.at(&to).of_box(new), part);
  }

  /// Edges fade like boxes, and channels ease only when they line up.
  #[test]
  fn edges_fade_and_their_channels_ease() {
    let a = NodeId::new();
    let (kept, new, gone, rerouted) =
      (EdgeId::new(), EdgeId::new(), EdgeId::new(), EdgeId::new());
    let c = |x| Channel {
      x,
      top: 0.0,
      bottom: 40.0,
    };
    let from = Drawn::settled(Placed {
      rects: [(a, Rect::new(0.0, 0.0, 10.0, 10.0))].into(),
      edges: [kept, gone, rerouted].into(),
      channels: [(kept, vec![c(0.0)]), (rerouted, vec![c(0.0)])].into(),
    });
    let to = Placed {
      rects: [(a, Rect::new(0.0, 0.0, 10.0, 10.0))].into(),
      edges: [kept, new, rerouted].into(),
      channels: [(kept, vec![c(100.0)]), (rerouted, vec![c(100.0), c(100.0)])]
        .into(),
    };
    let mut tween = Tween::between(from, &to).unwrap();
    tween.advance(50_000_000);
    let now = tween.at(&to);
    assert!(now.of_edge(new) > 0.0 && now.of_edge(new) < 1.0);
    assert!(now.of_edge(gone) > 0.0 && now.of_edge(gone) < 1.0);
    assert!(
      now.placed.edges.contains(&gone),
      "still drawn while it fades"
    );
    assert_eq!(now.of_edge(kept), 1.0);
    let x = now.placed.channels[&kept][0].x;
    assert!(x > 0.0 && x < 100.0);
    assert_eq!(now.placed.channels[&rerouted], to.channels[&rerouted]);
  }

  /// A long stall counts as one short step, so the motion is still seen.
  #[test]
  fn a_stall_does_not_skip_the_motion() {
    let a = NodeId::new();
    let to = placed(&[(a, Rect::new(100.0, 0.0, 200.0, 40.0))]);
    let from = placed(&[(a, Rect::new(0.0, 0.0, 100.0, 40.0))]);
    let mut tween = Tween::between(Drawn::settled(from), &to).unwrap();
    assert!(!tween.advance(2_000_000_000));
    assert!(tween.at(&to).placed.rects[&a].x0 < 100.0);
  }

  /// Nothing moves, arrives or leaves, or nothing was drawn before: there
  /// is nothing to ease.
  #[test]
  fn nothing_to_ease() {
    let a = NodeId::new();
    let r = Rect::new(0.0, 0.0, 100.0, 40.0);
    let to = placed(&[(a, r)]);
    assert!(Tween::between(Drawn::settled(to.clone()), &to).is_none());
    assert!(Tween::between(Drawn::default(), &to).is_none());
  }
}
