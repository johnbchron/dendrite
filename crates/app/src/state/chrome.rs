//! The chrome around the canvas: the palette (colour theme), popovers, the
//! inspector's width, and the camera requests the canvas acts on.

use super::AppState;
use crate::{
  camera::{Camera, CameraRequest, Insets, ZoomStep},
  focus::FocusRequests,
  theme::Theme,
  tokens::{size, space},
};

/// `meta` key the chosen palette is stored under.
const THEME_KEY: &str = "palette";

/// Default width of the inspector card, in logical pixels.
pub(super) const INSPECTOR_WIDTH: f64 = 320.0;
/// How narrow and how wide the inspector card may be dragged.
pub(super) const INSPECTOR_MIN: f64 = 280.0;
pub(super) const INSPECTOR_MAX: f64 = 560.0;

/// A popover, or the command palette; at most one is open at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Popover {
  /// The settings popover (the palette picker).
  Settings,
  /// The quest switcher.
  Quests,
  /// The command palette.
  Palette,
}

/// The inspector's width, resized by dragging its divider.
#[derive(Clone, Copy, Debug)]
pub(super) struct PanelWidth {
  /// Current width, in logical pixels.
  width: f64,
  /// Width when the current divider drag started, so drags measure against
  /// a fixed anchor instead of accumulating.
  base:  f64,
}

impl Default for PanelWidth {
  fn default() -> Self {
    Self {
      width: INSPECTOR_WIDTH,
      base:  INSPECTOR_WIDTH,
    }
  }
}

impl PanelWidth {
  /// Anchor a drag at the current width.
  fn begin_resize(&mut self) { self.base = self.width; }

  /// Resize from a drag whose pointer has travelled `dx` since the press;
  /// dragging left (negative) widens the panel.
  fn resize(&mut self, dx: f64) {
    self.width = (self.base - dx).clamp(INSPECTOR_MIN, INSPECTOR_MAX);
  }
}

impl AppState {
  /// The active palette. Every colour in the canvas and the panel comes from
  /// here.
  pub fn theme(&self) -> &'static Theme { self.theme }

  /// The palette the store records, or the default when it records none
  /// (or one this build no longer ships).
  pub(super) fn stored_theme(&self) -> &'static Theme {
    // A palette recorded by an older version that no longer ships falls back
    // to the default rather than blocking startup.
    let stored = match self.lock().setting(THEME_KEY) {
      Ok(id) => id,
      Err(e) => {
        eprintln!("reading the stored palette failed: {e}");
        None
      }
    };
    stored
      .and_then(|id| Theme::by_id(&id))
      .unwrap_or(Theme::DEFAULT)
  }

  /// Switch palettes and persist the choice. A failed write is reported and
  /// the palette still changes for this session.
  pub fn set_theme(&mut self, id: &str) {
    let Some(next) = Theme::by_id(id) else { return };
    self.theme = next;
    if let Err(e) = self.lock().set_setting(THEME_KEY, next.id) {
      eprintln!("saving the palette failed: {e}");
    }
  }

  /// Whether the settings popover is open.
  pub fn settings_open(&self) -> bool {
    self.popover == Some(Popover::Settings)
  }

  /// Open or close the settings popover, closing any other popover.
  pub fn toggle_settings(&mut self) {
    let open = !self.settings_open();
    self.close_popovers();
    if open {
      self.popover = Some(Popover::Settings);
    }
  }

  /// Whether any popover (or the palette) is open, so Escape closes it.
  pub fn popover_open(&self) -> bool { self.popover.is_some() }

  /// Whether a popover that a click anywhere else should close is open.
  pub fn dismissable_open(&self) -> bool {
    matches!(self.popover, Some(Popover::Settings | Popover::Quests))
  }

  /// Close every popover, and the palette.
  pub fn close_popovers(&mut self) { self.popover = None; }

  /// Close `popover`, if it is the one open.
  pub(super) fn close(&mut self, popover: Popover) {
    if self.popover == Some(popover) {
      self.popover = None;
    }
  }

  /// Whether the inspector's "more actions" list is showing.
  pub fn more_open(&self) -> bool { self.more_open }

  /// Show or hide the inspector's "more actions" list.
  pub fn toggle_more(&mut self) { self.more_open = !self.more_open; }

  /// The latest camera request, handed to the canvas view.
  pub fn camera(&self) -> Camera { self.camera }

  /// Ask the canvas camera to do something on the next rebuild.
  pub(super) fn aim(&mut self, request: CameraRequest) {
    self.camera = self.camera.then(request);
  }

  /// Ask the canvas to refit/centre the whole graph on the next frame.
  pub fn recenter(&mut self) { self.aim(CameraRequest::Fit); }

  /// Step the canvas zoom.
  pub fn zoom(&mut self, step: ZoomStep) {
    self.aim(CameraRequest::Zoom(step));
  }

  /// The canvas zoom as a whole percentage.
  pub fn zoom_percent(&self) -> u32 { self.zoom_percent }

  /// Record the zoom level the canvas reports.
  pub fn set_zoom_percent(&mut self, percent: u32) {
    self.zoom_percent = percent;
  }

  /// How much of the canvas the floating chrome covers, so fitting and
  /// revealing aim at the visible part.
  pub fn canvas_insets(&self) -> Insets {
    // The inspector card is only up while something is selected.
    let card = if self.selected.is_some() {
      self.panel.width + size::DIVIDER + 2.0 * space::M
    } else {
      0.0
    };
    Insets {
      top:    size::TOP_BAR,
      right:  card,
      bottom: 0.0,
      left:   0.0,
    }
  }

  /// The inspector's current width in logical pixels.
  pub fn inspector_width(&self) -> f64 { self.panel.width }

  /// Anchor a divider drag at the current panel width.
  pub fn begin_inspector_resize(&mut self) { self.panel.begin_resize(); }

  /// Resize the panel from a divider drag. `dx` is the pointer's total travel
  /// since the press, so dragging left (negative) widens the panel.
  pub fn resize_inspector(&mut self, dx: f64) { self.panel.resize(dx); }

  /// The handle the driver serves focus requests through.
  pub fn focus_requests(&self) -> FocusRequests { self.focus_requests.clone() }
}
