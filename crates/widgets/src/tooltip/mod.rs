//! Tooltips: a short label that appears under a control after the pointer
//! rests on it.
//!
//! The tooltip is drawn in a Masonry *layer* — a separate widget tree above
//! the whole window — so it is never clipped by the bar or card its control
//! sits in. [`TooltipWidget`] wraps the control, waits `DELAY_MS` of
//! hovering, measures the text, and places the layer centred under the
//! control (or right-aligned, for controls at the window's right edge).
//!
//! How it is drawn comes in as a [`Look`]; the palette is the app's.

mod bubble;
mod view;
mod widget;

use masonry::{parley::style::FontStack, peniko::Color, properties::Padding};

pub use self::{
  view::{Tooltip, tooltip},
  widget::TooltipWidget,
};

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

/// How a tooltip is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
  /// The bubble's ground.
  pub ground:    Color,
  /// Its 1px border.
  pub border:    Color,
  /// The label.
  pub text:      Color,
  /// Corner radius.
  pub radius:    f64,
  /// Label size, in logical pixels.
  pub text_size: f32,
  /// Space between the label and the bubble's edge.
  pub padding:   Padding,
  /// The face the label is shaped in.
  pub font:      FontStack<'static>,
}
