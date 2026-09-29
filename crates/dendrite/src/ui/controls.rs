//! The chrome's building blocks: labels in the app face, the button styles,
//! chips and section markers. Everything here takes the theme explicitly,
//! since Masonry's defaults assume a dark palette.

use base::NodeState;
use masonry::{
  core::ArcStr,
  peniko::Color,
  properties::{LineBreaking, Padding, types::AsUnit},
};
use xilem::{
  FontWeight, WidgetView,
  style::Style as _,
  view::{
    CrossAxisAlignment, FlexExt as _, FlexItem, FlexSequence, Label, button,
    flex_col, flex_row, sized_box,
  },
};

use crate::{
  font,
  icons::{Icon, icon_label},
  state::AppState,
  theme::Theme,
  tokens::{radius, size, space, text},
};

/// Every label in the chrome, in the app's typeface. This shadows xilem's
/// `label` on purpose: that one sets an empty font stack, which throws away
/// even Masonry's default family (see [`font`]).
pub(super) fn label(text: impl Into<ArcStr>) -> Label {
  xilem::view::label(text).font(font::STACK)
}

/// A button's label: the button scale, the palette's text colour, and muted
/// when the button is disabled (Masonry's default disabled text is a fixed
/// grey, which vanishes on a light palette).
pub(super) fn btn_label<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  label(text.into())
    .text_size(text::CONTROL)
    .color(theme.text)
    .disabled_color(theme.muted)
}

/// The one filled button on a surface: the action it leads with. Disabled,
/// it drops to the plain ground so it no longer reads as the thing to do.
pub(super) fn primary_btn<S, F>(
  text: S,
  theme: &'static Theme,
  enabled: bool,
  on_press: F,
) -> impl WidgetView<AppState> + use<S, F>
where
  S: Into<String>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  button(
    label(text.into())
      .text_size(text::CONTROL)
      .weight(FontWeight::SEMI_BOLD)
      .color(theme.on_accent)
      .disabled_color(theme.muted),
    on_press,
  )
  .disabled(!enabled)
  .padding(Padding::from_vh(space::CONTROL_Y, space::M + space::XS))
  .corner_radius(radius::CONTROL)
  .background_color(theme.accent)
  .active_background_color(theme.focus)
  .disabled_background_color(theme.sunken)
  .border_color(Color::TRANSPARENT)
  .hovered_border_color(theme.text)
}

/// A destructive action, outlined in the palette's alarm colour so it
/// never reads as the thing to do, but stays in plain sight.
pub(super) fn danger_btn<S, F>(
  text: S,
  theme: &'static Theme,
  on_press: F,
) -> impl WidgetView<AppState> + use<S, F>
where
  S: Into<String>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  button(
    flex_row((
      icon_label(Icon::Trash, size::ICON).color(theme.cycle),
      label(text.into())
        .text_size(text::CONTROL)
        .color(theme.cycle),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::XS.px()),
    on_press,
  )
  .padding(Padding::from_vh(space::CONTROL_Y, space::CONTROL_X))
  .corner_radius(radius::CONTROL)
  .background_color(Color::TRANSPARENT)
  .active_background_color(theme.sunken)
  .border_color(theme.rule)
  .hovered_border_color(theme.cycle)
}

/// One segment of a [`group`]: flat, so the group's frame carries the shape
/// and the segments read as one control. `active` marks the current choice
/// in a picker; `enabled` greys out a command that has nothing to do.
pub(super) fn seg<S, F>(
  text: S,
  theme: &'static Theme,
  active: bool,
  enabled: bool,
  on_press: F,
) -> impl WidgetView<AppState> + use<S, F>
where
  S: Into<String>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  let ground = if active {
    theme.rule
  } else {
    Color::TRANSPARENT
  };
  button(btn_label(text, theme), on_press)
    .disabled(!enabled)
    .padding(Padding::from_vh(space::CONTROL_Y, space::CONTROL_X))
    .corner_radius(radius::CONTROL)
    .background_color(ground)
    .active_background_color(theme.rule)
    .disabled_background_color(Color::TRANSPARENT)
    // A transparent border that lights up on hover, so hovering does not
    // shift the label by a pixel.
    .border_color(Color::TRANSPARENT)
    .hovered_border_color(theme.accent)
}

/// Declaring free time in one press: 30 minutes, an hour or two from now,
/// or none.
pub(super) fn free_presets(
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  flex_row((
    group(
      theme,
      (
        seg("30 min", theme, false, true, |s: &mut AppState| {
          s.set_free_for(30)
        }),
        seg("1 h", theme, false, true, |s: &mut AppState| {
          s.set_free_for(60)
        }),
        seg("2 h", theme, false, true, |s: &mut AppState| {
          s.set_free_for(120)
        }),
      ),
    ),
    seg("Clear", theme, false, true, |s: &mut AppState| {
      s.set_free_until(None)
    }),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::XS.px())
}

/// A flat icon-only button, sized like a [`seg`] so it can sit in a
/// [`group`] beside them. `active` fills it, as for a toggle that is on.
pub(super) fn icon_btn<F>(
  glyph: Icon,
  theme: &'static Theme,
  active: bool,
  enabled: bool,
  on_press: F,
) -> impl WidgetView<AppState> + use<F>
where
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  let ground = if active {
    theme.rule
  } else {
    Color::TRANSPARENT
  };
  button(
    icon_label(glyph, size::ICON)
      .color(theme.text)
      .disabled_color(theme.rule),
    on_press,
  )
  .disabled(!enabled)
  .padding(Padding::from_vh(space::CONTROL_Y, space::CONTROL_X))
  .corner_radius(radius::CONTROL)
  .background_color(ground)
  .active_background_color(theme.rule)
  .disabled_background_color(Color::TRANSPARENT)
  .border_color(Color::TRANSPARENT)
  .hovered_border_color(theme.accent)
}

/// A row of [`seg`]s framed as one segmented control: related commands sit
/// together, and the toolbar reads as a few groups rather than a run of
/// identical boxes.
pub(super) fn group<Seq>(
  theme: &'static Theme,
  segments: Seq,
) -> impl WidgetView<AppState> + use<Seq>
where
  Seq: FlexSequence<AppState> + Send + Sync,
{
  sized_box(
    flex_row(segments)
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::HAIR.px()),
  )
  .padding(Padding::all(space::HAIR))
  .corner_radius(radius::CONTROL + space::HAIR)
  .background_color(theme.sunken)
  .border_color(theme.rule)
  .border_width(1.0)
}

/// A list entry that acts on press — a quest to switch to, a node to jump
/// to. Left-aligned and flat, so a stack of them reads as a list rather than
/// a column of centred buttons; `active` marks the current (or highlighted)
/// one.
pub(super) fn row_button<V, F>(
  theme: &'static Theme,
  active: bool,
  content: V,
  on_press: F,
) -> impl WidgetView<AppState> + use<V, F>
where
  V: WidgetView<AppState>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  let ground = if active {
    theme.sunken
  } else {
    Color::TRANSPARENT
  };
  // Filling the width inside the button is what pins the content left: the
  // button centres its child, and a full-width child has nowhere to go.
  button(sized_box(content).expand_width(), on_press)
    .padding(Padding::from_vh(space::CONTROL_Y, space::CONTROL_X))
    .corner_radius(radius::CONTROL)
    .background_color(ground)
    .active_background_color(theme.rule)
    .border_color(Color::TRANSPARENT)
    .hovered_border_color(theme.accent)
}

/// Flexible empty space in a row: pushes what follows it to the far end.
/// (A flex factor on a label only allots it room; the label stays as wide
/// as its text, so it cannot push anything.)
pub(super) fn spacer()
-> FlexItem<impl WidgetView<AppState> + use<>, AppState, ()> {
  sized_box(flex_col(())).expand_width().flex(1.0)
}

/// `view` taking all the room left in a row: a label in it wraps at the
/// row's edge instead of overflowing it, and anything after it is pushed to
/// the row's far end.
pub(super) fn fill<V>(
  view: V,
) -> FlexItem<impl WidgetView<AppState> + use<V>, AppState, ()>
where
  V: WidgetView<AppState>,
{
  sized_box(view).expand_width().flex(1.0)
}

/// A small round swatch of `color`, for previews such as the palette list.
/// Ringed in `theme.rule`, so a swatch the colour of the ground still shows.
pub(super) fn swatch(
  color: Color,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  sized_box(flex_col(()))
    .width(12.0.px())
    .height(12.0.px())
    .corner_radius(radius::PILL)
    .background_color(color)
    .border_color(theme.rule)
    .border_width(1.0)
}

/// A node's state as a small dot in its border colour, as the canvas
/// draws it.
pub(super) fn state_dot(
  state: NodeState,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let (fill, border) = theme.for_state(state);
  sized_box(flex_col(()))
    .width(10.0.px())
    .height(10.0.px())
    .corner_radius(radius::PILL)
    .background_color(fill)
    .border_color(border)
    .border_width(2.0)
}

/// Human-readable label for a derived node state.
pub(super) fn state_str(state: NodeState) -> &'static str {
  match state {
    NodeState::Completed => "Completed",
    NodeState::Ready => "Ready",
    NodeState::Blocked => "Blocked",
    NodeState::Cyclic => "Cyclic",
    NodeState::Satisfied => "Satisfied",
    NodeState::Pending => "Pending",
  }
}

/// A 1px hairline that fills the width it is given.
pub(super) fn rule(theme: &'static Theme) -> impl WidgetView<AppState> + use<> {
  sized_box(flex_col(()))
    .height(1.0.px())
    .expand_width()
    .background_color(theme.rule)
}

/// Primary chrome text. Masonry's default label colour is a fixed light grey,
/// which only works on a dark ground — every palette carries its own text
/// colour instead, and this is where it goes on.
pub(super) fn body<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  // Word-wrapped: in a row with room to fill (see [`fill`]) a long name
  // wraps at the edge. Masonry labels overflow by default.
  label(text.into())
    .text_size(text::BODY)
    .color(theme.text)
    .line_break_mode(LineBreaking::WordWrap)
}

/// A section marker: a small uppercase label with a hairline running out to
/// the right edge. This replaces the per-section cards, which cost 24px of
/// horizontal room apiece without carrying any information.
pub(super) fn section<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  flex_row((
    label(text.into().to_uppercase())
      .text_size(text::LABEL)
      .weight(FontWeight::BOLD)
      .color(theme.muted),
    rule(theme).flex(1.0),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::S.px())
}

/// Muted secondary text.
pub(super) fn muted<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  label(text.into())
    .text_size(text::SECONDARY)
    .color(theme.muted)
    .line_break_mode(LineBreaking::WordWrap)
}

/// A small coloured status chip.
pub(super) fn chip<S: Into<String>>(
  text: S,
  color: masonry::peniko::Color,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  sized_box(label(text.into()).text_size(text::LABEL).color(theme.text))
    .padding(Padding::from_vh(space::XS, space::CONTROL_X))
    .background_color(color)
    .corner_radius(radius::PILL)
}
