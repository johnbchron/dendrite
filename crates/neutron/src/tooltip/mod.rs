//! Tooltips: a short label that appears under a control after the pointer
//! rests on it.
//!
//! The tooltip is drawn in a Masonry *layer* — a separate widget tree above
//! the whole window — so it is never clipped by the bar or card its control
//! sits in. [`TooltipWidget`] wraps the control, waits [`DELAY_MS`] of
//! hovering, measures the text, and places the layer centred under the
//! control (or right-aligned, for controls at the window's right edge).

mod bubble;
mod view;
mod widget;

use masonry::peniko::Color;

pub use self::{view::tooltip, widget::TooltipWidget};
use crate::theme::Theme;

/// How long the pointer rests on a control before its tooltip shows.
const DELAY_MS: f64 = 450.0;
/// Gap between the control's lower edge and the tooltip.
const GAP: f64 = 6.0;

/// Where the tooltip sits relative to its control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
  /// Centred under the control.
  Center,
  /// Under the control, right edges aligned: for controls near the window's
  /// right edge, where a centred tooltip would run off it.
  End,
}

/// The tooltip's colours.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Colors {
  ground: Color,
  border: Color,
  text:   Color,
}

impl Colors {
  fn from_theme(theme: &Theme) -> Self {
    Self {
      ground: theme.surface_raised,
      border: theme.rule,
      text:   theme.text,
    }
  }
}
