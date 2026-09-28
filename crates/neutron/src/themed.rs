//! Neutron's palette and design tokens, bound to the generic [`widgets`].
//!
//! The widget crate knows nothing about this app: each of its views takes
//! the colours, sizes and font it should paint with. This module is the one
//! place that maps [`Theme`] and [`app::tokens`] onto them, so the views call
//! these wrappers and never see a `Look`.

use app::{
  focus::FieldKey,
  theme::Theme,
  tokens::{radius, size, space, text},
};
use masonry::{core::WidgetId, properties::Padding};
pub use widgets::{
  appear::{Motion, appear},
  divider::DividerAction,
  timer::after,
  tooltip::Anchor,
};
use widgets::{divider, field, hover_row, surface, tooltip};
use xilem::WidgetView;

/// How far a surface floats above the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
  /// A full-width bar along the top edge: flat, with a hairline beneath.
  Bar,
  /// A card resting on the canvas: rounded, bordered, a soft shadow.
  Card,
  /// A popover or palette above everything: the raised ground and a deeper
  /// shadow.
  Popover,
}

impl Level {
  /// What this elevation looks like in `theme`.
  fn style(self, theme: &Theme) -> surface::Style {
    let (ground, radius, shadow) = match self {
      Level::Bar => (theme.surface, 0.0, None),
      Level::Card => {
        (theme.surface, radius::CARD, Some((theme.shadow, 4.0, 8.0)))
      }
      Level::Popover => (
        theme.surface_raised,
        radius::CARD,
        Some((theme.shadow, 8.0, 16.0)),
      ),
    };
    surface::Style {
      ground,
      border: theme.rule,
      radius,
      shadow,
      flat: self == Level::Bar,
    }
  }
}

/// `child` on a surface of the given `level`, padded by `padding` logical
/// pixels on every side.
pub fn surface<State, Action, V>(
  theme: &'static Theme,
  level: Level,
  padding: f64,
  child: V,
) -> surface::Surface<V>
where
  V: WidgetView<State, Action>,
{
  surface::surface(level.style(theme), padding, child)
}

/// The draggable divider along the inspector's left edge.
pub fn divider<State, Action, F>(
  theme: &'static Theme,
  on_action: F,
) -> divider::Divider<
  impl Fn(&mut State, DividerAction) -> xilem::core::MessageResult<Action>,
>
where
  F: Fn(&mut State, DividerAction) -> Action + 'static,
{
  divider::divider(size::DIVIDER, theme.accent, on_action)
}

/// A row of `main`, with `trailing` shown at its end on hover.
pub fn hover_row<M, T>(main: M, trailing: T) -> hover_row::HoverRow<M, T> {
  hover_row::hover_row(space::XS, main, trailing)
}

/// `child` with a tooltip reading `text`.
pub fn tooltip<V>(
  label: impl Into<String>,
  theme: &'static Theme,
  anchor: Anchor,
  child: V,
) -> tooltip::Tooltip<V> {
  tooltip::tooltip(
    label,
    tooltip::Look {
      ground:    theme.surface_raised,
      border:    theme.rule,
      text:      theme.text,
      radius:    radius::CONTROL,
      text_size: text::SECONDARY,
      padding:   Padding::from_vh(space::XS, space::S),
      font:      crate::font::STACK,
    },
    anchor,
    child,
  )
}

/// A text field showing `contents`, reporting every edit to `on_changed`.
pub fn field<State, Action, F>(
  contents: String,
  theme: &'static Theme,
  on_changed: F,
) -> field::Field<State, Action>
where
  F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
{
  field::field(
    contents,
    field::Look {
      ground: theme.sunken,
      border: theme.rule,
      focus:  theme.focus,
      text:   theme.text,
      muted:  theme.muted,
      radius: radius::CONTROL,
      font:   crate::font::STACK,
    },
    text::BODY,
    on_changed,
  )
}

/// A field registered in [`crate::focus`] under the name the key map knows
/// it by, so a key can send focus there.
struct Registered(FieldKey);

impl field::Mount for Registered {
  fn mounted(&self, id: WidgetId) { crate::focus::register(self.0, id); }

  fn unmounted(&self, id: WidgetId) { crate::focus::unregister(self.0, id); }
}

/// Naming a field so the key map can focus it.
pub trait FocusKey {
  /// Let the key map focus this field by `key`.
  fn focus_key(self, key: FieldKey) -> Self;
}

impl<State, Action> FocusKey for field::Field<State, Action> {
  fn focus_key(self, key: FieldKey) -> Self { self.mount(Registered(key)) }
}
