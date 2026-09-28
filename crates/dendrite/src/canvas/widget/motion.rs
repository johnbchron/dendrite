//! Moving the camera: acting on the app's requests, and easing the view
//! towards where they point, a frame at a time.

use app::camera::{ZOOM_MAX, ZOOM_MIN};
use masonry::{
  core::UpdateCtx,
  kurbo::{Point, Rect, Size},
};

use super::CanvasWidget;
use crate::canvas::{
  CameraRequest, CanvasAction,
  frame::{Frame, ZOOM_RATE},
};

impl CanvasWidget {
  /// Act on a camera request from the app.
  pub(in crate::canvas) fn apply(&mut self, request: CameraRequest) {
    match request {
      // Eased, like a wheel zoom; the fits at startup and on a resize
      // (`needs_fit`) stay instant.
      CameraRequest::Fit => self.fit_pending = true,
      CameraRequest::Reveal(node) => self.reveal = Some(node),
      CameraRequest::Zoom(step) => {
        self.zoom_target = step.applied_to(self.zoom_target);
        let size = self.last_size.unwrap_or_default();
        self.zoom_anchor = self.insets.uncovered(size).center();
        self.glide = None;
      }
    }
  }

  /// Aim the next frames' zoom by the factor `exp(dy / 10)` about the
  /// screen point `anchor`: a wheel notch.
  pub(super) fn wheel(&mut self, anchor: Point, dy: f64) {
    self.zoom_target =
      (self.zoom_target * (dy * 0.1).exp()).clamp(ZOOM_MIN, ZOOM_MAX);
    self.zoom_anchor = anchor;
    self.glide = None;
  }

  /// Every placed box, unioned; `None` before anything is placed.
  fn bounds(&self) -> Option<Rect> {
    self.rects.values().copied().reduce(|a, b| a.union(b))
  }

  /// Fit the whole scene into the part of `viewport` the chrome leaves
  /// uncovered, at once.
  pub(super) fn fit_to(&mut self, viewport: Size) {
    let Some(bounds) = self.bounds() else {
      return;
    };
    let Some(fitted) = Frame::fit(bounds, self.insets.uncovered(viewport))
    else {
      return;
    };
    self.frame = fitted;
    // A fit replaces the view outright, so drop any motion still in flight.
    self.zoom_target = fitted.zoom;
    self.glide = None;
  }

  /// The zoom level as a whole percentage, if it differs from the one last
  /// reported (and records it as reported).
  pub(super) fn zoom_change(&mut self) -> Option<u32> {
    let percent = (self.frame.zoom * 100.0).round() as u32;
    (percent != self.reported_zoom).then(|| {
      self.reported_zoom = percent;
      percent
    })
  }

  /// Start easing towards a pending fit, once there are boxes to fit.
  fn start_fit(&mut self) {
    if !self.fit_pending {
      return;
    }
    let (Some(bounds), Some(size)) = (self.bounds(), self.last_size) else {
      return;
    };
    self.fit_pending = false;
    if let Some(target) = Frame::fit(bounds, self.insets.uncovered(size)) {
      self.glide = Some(target);
    }
  }

  /// Start easing towards a pending [`Self::reveal`], if its node is placed
  /// and not already in view: a pan at the zoom of the moment, which stops
  /// any zoom still in flight. Returns whether it resolved the request.
  fn start_reveal(&mut self) -> bool {
    let (Some(node), Some(size)) = (self.reveal, self.last_size) else {
      return false;
    };
    let Some(&rect) = self.rects.get(&node) else {
      return false;
    };
    self.reveal = None;
    if let Some(target) = self.frame.reveal(rect, self.insets.uncovered(size)) {
      self.glide = Some(target);
    }
    true
  }

  /// One animation frame, `interval` nanoseconds after the last: start any
  /// pending fit or reveal, then ease the view a step towards its target.
  pub(super) fn animate(&mut self, ctx: &mut UpdateCtx<'_>, interval: u64) {
    // Ease in log space, so zooming in and out feel the same speed. A long
    // stall (the first frame, or a hitch) is capped so it cannot overshoot.
    let dt = (interval as f64 / 1e9).min(0.1);
    let ease = 1.0 - (-ZOOM_RATE * dt).exp();
    self.start_reveal();
    self.start_fit();

    let moving = if let Some(target) = self.glide {
      // A fit or reveal moves zoom and pan together, straight to its view;
      // the anchored zoom below would drag the pan off course.
      let (next, arrived) = self.frame.eased_toward(target, ease);
      self.frame = next;
      self.zoom_target = next.zoom;
      if arrived {
        self.glide = None;
      }
      !arrived
    } else {
      let gap = (self.zoom_target / self.frame.zoom).ln();
      let step = if gap.abs() < 1e-3 { gap } else { gap * ease };
      self.frame = self.frame.zoomed_about(self.zoom_anchor, step.exp());
      (self.zoom_target / self.frame.zoom).ln().abs() >= 1e-3
    };

    if let Some(percent) = self.zoom_change() {
      ctx.submit_action::<CanvasAction>(CanvasAction::Zoomed(percent));
    }
    ctx.request_render();
    if moving || self.reveal.is_some() || self.fit_pending {
      ctx.request_anim_frame();
    }
  }
}
