//! The world→screen transform the canvas moves, and the maths behind
//! fitting, revealing and easing it.
//!
//! Split from [`app::camera`], which holds what the *app* says about the
//! camera. Everything here needs a viewport, so it belongs to the widget.

use app::camera::{ZOOM_MAX, ZOOM_MIN, ZOOM_RESET};
use masonry::kurbo::{Affine, Point, Rect, Size, Vec2};

/// How quickly an animated zoom closes on its target, per second: the gap
/// shrinks by a factor of `e` every `1 / ZOOM_RATE` seconds, so a wheel notch
/// settles in roughly a fifth of a second and rapid notches blend together
/// rather than stepping.
pub(crate) const ZOOM_RATE: f64 = 16.0;
/// Screen pixels kept clear around the graph when fitting it, and around a
/// node revealed by a reveal request.
const VIEW_MARGIN: f64 = 48.0;
/// Screen pixels beyond the viewport that still count as on screen when
/// culling, so a selection outline or an edge's stroke that pokes past a
/// box is never cut off at the window edge.
const CULL_MARGIN: f64 = 16.0;

/// A world→screen transform: scale by `zoom`, then translate by `pan`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
  /// World→screen scale.
  pub(crate) zoom: f64,
  /// World→screen translation.
  pub(crate) pan: Vec2,
}

impl Frame {
  /// The transform as an affine map.
  pub(crate) fn affine(self) -> Affine {
    Affine::translate(self.pan) * Affine::scale(self.zoom)
  }

  /// Map a screen point (widget-local) to world coordinates.
  pub(crate) fn to_world(self, screen: Point) -> Point {
    self.affine().inverse() * screen
  }

  /// The world-space area a `size`-sized viewport shows, plus
  /// [`CULL_MARGIN`].
  pub(crate) fn visible_world(self, size: Size) -> Rect {
    let screen = size.to_rect().inflate(CULL_MARGIN, CULL_MARGIN);
    self.affine().inverse().transform_rect_bbox(screen)
  }

  /// The frame that fits world-space `bounds` into screen-space `view`,
  /// centred with a [`VIEW_MARGIN`]. Never zooms in past 1:1, so a small
  /// graph stays readable rather than filling the window with a few giant
  /// boxes. `None` for an empty view.
  pub(crate) fn fit(bounds: Rect, view: Rect) -> Option<Self> {
    if view.width() <= 0.0 || view.height() <= 0.0 {
      return None;
    }
    let avail_w = (view.width() - 2.0 * VIEW_MARGIN).max(1.0);
    let avail_h = (view.height() - 2.0 * VIEW_MARGIN).max(1.0);
    let zoom = (avail_w / bounds.width().max(1.0))
      .min(avail_h / bounds.height().max(1.0))
      .min(ZOOM_RESET)
      .clamp(ZOOM_MIN, ZOOM_MAX);
    let pan = view.center().to_vec2() - zoom * bounds.center().to_vec2();
    Some(Self { zoom, pan })
  }

  /// The frame, at this zoom, that brings world-space `node` into
  /// screen-space `view`: `None` if it is already inside `view` with a
  /// [`VIEW_MARGIN`] to spare, else the one that centres it.
  pub(crate) fn reveal(self, node: Rect, view: Rect) -> Option<Self> {
    let shown = self.affine().transform_rect_bbox(node);
    let comfortable = view.inset(-VIEW_MARGIN);
    let inside = comfortable.width() > 0.0
      && comfortable.height() > 0.0
      && comfortable.contains(shown.origin())
      && comfortable.contains(Point::new(shown.x1, shown.y1));
    (!inside).then(|| Self {
      zoom: self.zoom,
      pan: view.center().to_vec2() - self.zoom * node.center().to_vec2(),
    })
  }

  /// This frame scaled by `factor` about screen point `anchor`, keeping the
  /// world point under it fixed, within the zoom limits.
  pub(crate) fn zoomed_about(self, anchor: Point, factor: f64) -> Self {
    let world = self.to_world(anchor);
    let zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    // Solve pan so that the transform takes `world` to `anchor` again.
    let pan = anchor.to_vec2() - zoom * world.to_vec2();
    Self { zoom, pan }
  }

  /// One animation step towards `target` by the fraction `ease`: zoom in log
  /// space, so zooming in and out feel the same speed, and pan in screen
  /// pixels. Snaps onto the target once within a hair of it, and says
  /// whether it has arrived.
  pub(crate) fn eased_toward(self, target: Self, ease: f64) -> (Self, bool) {
    let zoom_gap = (target.zoom / self.zoom).ln();
    let pan_gap = target.pan - self.pan;
    if zoom_gap.abs() < 1e-3 && pan_gap.hypot() < 0.5 {
      return (target, true);
    }
    let next = Self {
      zoom: self.zoom * (zoom_gap * ease).exp(),
      pan: self.pan + pan_gap * ease,
    };
    (next, false)
  }
}

#[cfg(test)]
mod tests {
  use app::camera::Insets;

  use super::*;

  fn frame(zoom: f64, pan: Vec2) -> Frame {
    Frame { zoom, pan }
  }

  /// Culling works in world space: the visible area follows pan and zoom,
  /// with a margin so outlines at the window edge are kept.
  #[test]
  fn visible_world_follows_pan_and_zoom() {
    let size = Size::new(800.0, 600.0);
    let m = CULL_MARGIN;
    let identity = frame(1.0, Vec2::ZERO).visible_world(size);
    assert_eq!(identity, Rect::new(-m, -m, 800.0 + m, 600.0 + m));

    // Zoomed to 2x and panned: the view covers half the world, offset.
    let v = frame(2.0, Vec2::new(100.0, 50.0)).visible_world(size);
    assert_eq!(
      v,
      Rect::new(
        (-m - 100.0) / 2.0,
        (-m - 50.0) / 2.0,
        (800.0 + m - 100.0) / 2.0,
        (600.0 + m - 50.0) / 2.0,
      )
    );
    assert!(Rect::new(0.0, 0.0, 10.0, 10.0).overlaps(v));
    assert!(!Rect::new(1000.0, 0.0, 1100.0, 50.0).overlaps(v));
  }

  /// Fitting aims at the uncovered area: the graph's centre lands on its
  /// centre, not the window's.
  #[test]
  fn fit_centres_the_graph_in_the_uncovered_area() {
    let viewport = Size::new(1000.0, 800.0);
    let insets = Insets {
      top: 40.0,
      right: 320.0,
      bottom: 0.0,
      left: 0.0,
    };
    let view = insets.uncovered(viewport);
    assert_eq!(view, Rect::new(0.0, 40.0, 680.0, 800.0));
    let bounds = Rect::new(-100.0, -50.0, 100.0, 50.0);
    let fitted = Frame::fit(bounds, view).unwrap();
    assert_eq!(fitted.zoom, ZOOM_RESET, "a small graph is not blown up");
    assert_eq!(fitted.affine() * bounds.center(), view.center());

    // A big graph shrinks to fit inside the margins (down to the minimum
    // zoom, which this one does not reach).
    let big = Rect::new(0.0, 0.0, 2000.0, 1000.0);
    let shown = Frame::fit(big, view)
      .unwrap()
      .affine()
      .transform_rect_bbox(big);
    assert!(shown.x0 >= view.x0 + VIEW_MARGIN - 1e-9);
    assert!(shown.x1 <= view.x1 - VIEW_MARGIN + 1e-9);
  }

  /// Insets larger than the canvas leave an empty view, not an inverted one,
  /// and fitting into it does nothing.
  #[test]
  fn oversized_insets_leave_an_empty_view() {
    let insets = Insets {
      top: 0.0,
      right: 900.0,
      bottom: 0.0,
      left: 200.0,
    };
    let view = insets.uncovered(Size::new(1000.0, 800.0));
    assert_eq!(view.width(), 0.0);
    assert!(Frame::fit(Rect::new(0.0, 0.0, 10.0, 10.0), view).is_none());
  }

  /// Revealing leaves a node that is already comfortably in view alone, and
  /// centres one that is off screen or under the chrome.
  #[test]
  fn reveal_only_moves_for_hidden_nodes() {
    let view = Rect::new(0.0, 40.0, 680.0, 800.0);
    let at = frame(ZOOM_RESET, Vec2::ZERO);
    let visible = Rect::new(200.0, 200.0, 350.0, 250.0);
    assert_eq!(at.reveal(visible, view), None);

    // Under the inspector card, off to the right of the view.
    let covered = Rect::new(700.0, 200.0, 850.0, 250.0);
    let target = at.reveal(covered, view).unwrap();
    assert_eq!(target.zoom, at.zoom, "a reveal only pans");
    assert_eq!(target.affine() * covered.center(), view.center());
  }

  /// An eased fit closes on the fitted view a step at a time, zoom and pan
  /// together, and lands on it exactly.
  #[test]
  fn fits_ease_zoom_and_pan_together() {
    let target = frame(ZOOM_RESET, Vec2::new(300.0, 200.0));
    let mut camera = frame(3.0, Vec2::new(-500.0, 40.0));
    let (first, arrived) = camera.eased_toward(target, 0.25);
    assert!(!arrived);
    assert!(
      first.zoom < 3.0 && first.zoom > ZOOM_RESET,
      "zoom moves part of the way"
    );
    assert!(first.pan.x > -500.0 && first.pan.x < 300.0, "pan does too");
    let mut steps = 0;
    loop {
      let (next, arrived) = camera.eased_toward(target, 0.25);
      camera = next;
      steps += 1;
      if arrived {
        break;
      }
      assert!(steps < 200, "never arrived");
    }
    assert_eq!(camera, target, "lands exactly");
  }
}
