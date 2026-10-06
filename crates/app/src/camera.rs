//! The camera, as the app asks about it: what it can be told to do, how
//! much of it the chrome covers, and the limits it works within.
//!
//! The world→screen transform itself belongs to whatever draws the canvas,
//! which is the only thing that knows the viewport.

use base::NodeId;
use kurbo::{Rect, Size};

/// How far the view zooms in and out.
pub const ZOOM_MIN: f64 = 0.10;
pub const ZOOM_MAX: f64 = 16.0;
/// The zoom level the view resets to, and starts at.
pub const ZOOM_RESET: f64 = 1.2;
/// How much one [`ZoomStep`] multiplies or divides the zoom by.
const ZOOM_STEP: f64 = 1.25;

/// How much of the canvas the chrome floating over it covers on each side,
/// in logical pixels. Fitting and revealing aim at the area left uncovered.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
  /// Covered from the top edge.
  pub top: f64,
  /// Covered from the right edge.
  pub right: f64,
  /// Covered from the bottom edge.
  pub bottom: f64,
  /// Covered from the left edge.
  pub left: f64,
}

impl Insets {
  /// The part of a `viewport`-sized canvas these leave uncovered, in screen
  /// coordinates. Never inverted, however large the insets.
  pub fn uncovered(self, viewport: Size) -> Rect {
    let x0 = self.left.min(viewport.width);
    let y0 = self.top.min(viewport.height);
    Rect::new(
      x0,
      y0,
      (viewport.width - self.right).max(x0),
      (viewport.height - self.bottom).max(y0),
    )
  }
}

/// Something the app asks the canvas's camera to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraRequest {
  /// Fit the whole graph into the uncovered area.
  Fit,
  /// Pan, easing, until the box is inside the uncovered area. A box that
  /// is already comfortably in view does not move. A node's own id names
  /// its first (or only) copy.
  Reveal(NodeId),
  /// Step the zoom, easing, about the centre of the uncovered area.
  Zoom(ZoomStep),
}

/// `zoom` as the whole percentage shown for it.
pub fn zoom_percent(zoom: f64) -> u32 {
  (zoom * 100.0).round() as u32
}

/// A zoom step from the zoom controls or keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomStep {
  /// One step closer.
  In,
  /// One step further out.
  Out,
  /// Back to [`ZOOM_RESET`].
  Reset,
}

impl ZoomStep {
  /// The zoom one step on from `zoom`, within the zoom limits.
  pub fn applied_to(self, zoom: f64) -> f64 {
    match self {
      ZoomStep::In => zoom * ZOOM_STEP,
      ZoomStep::Out => zoom / ZOOM_STEP,
      ZoomStep::Reset => ZOOM_RESET,
    }
    .clamp(ZOOM_MIN, ZOOM_MAX)
  }
}

/// The latest camera request, tagged with a counter the app bumps for each
/// new one, so repeating the same request (Fit twice) still acts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
  /// Bumped per request.
  pub epoch: u64,
  /// What to do.
  pub request: CameraRequest,
}

impl Default for Camera {
  /// The request a fresh app starts with: fit the graph.
  fn default() -> Self {
    Self {
      epoch: 0,
      request: CameraRequest::Fit,
    }
  }
}

impl Camera {
  /// The camera after `request`, with a fresh epoch so it acts even when it
  /// repeats the last one.
  pub fn then(self, request: CameraRequest) -> Self {
    Self {
      epoch: self.epoch + 1,
      request,
    }
  }
}
