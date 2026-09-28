//! The reason line: why the selection is in its state.

use masonry::properties::types::AsUnit;
use xilem::{
  WidgetView,
  style::Style as _,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row},
};

use super::icon_row;
use crate::{
  icons::{Icon, icon},
  state::{AppState, Reason},
  theme::Theme,
  tokens::{size, space, text},
  ui::controls::{body, label, muted},
};

/// Why the node is in its state: one sentence, then — when other nodes are
/// the reason — each of them as a row that goes to it, with the facts
/// behind a formula condition under its name.
pub(super) fn reason_line(
  reason: &Reason,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let alert = reason.is_alert();
  let rows: Vec<_> = reason
    .nodes()
    .iter()
    .map(|named| {
      let id = named.node;
      icon_row(
        theme,
        false,
        Icon::ArrowRight,
        theme.muted,
        flex_col((
          body(named.name.clone(), theme),
          named.why.clone().map(|why| muted(why, theme)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(0.0.px()),
        move |s: &mut AppState| s.go_to(id),
      )
      .into_any_flex()
    })
    .collect();
  flex_col((
    flex_row((
      alert.then(|| icon(Icon::CircleAlert, size::ICON, theme.cycle)),
      label(reason.sentence())
        .text_size(text::BODY)
        .color(if alert { theme.cycle } else { theme.muted }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
    flex_col(rows)
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::HAIR.px()),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Fill)
  .gap(space::XS.px())
}
