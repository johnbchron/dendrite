//! The slim window-wide toolbar. These commands act on the document, not on
//! the selection, so they belong above both panes rather than in the panel.

use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  FontWeight, WidgetView,
  style::Style as _,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, sized_box},
};

use super::controls::{group, label, muted, seg};
use crate::{
  state::AppState,
  tokens::{space, text},
};

/// The slim window-wide toolbar. These commands act on the document, not on
/// the selection, so they belong above both panes rather than in the panel.
pub(super) fn toolbar(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let open = data.palette_open();

  let commands = flex_row((
    label("Neutron")
      .text_size(text::BODY)
      .weight(FontWeight::BOLD)
      .color(theme.text)
      .flex(1.0),
    group(
      theme,
      (
        seg("+ Task", theme, false, true, |s: &mut AppState| {
          s.add_task()
        }),
        seg("+ Condition", theme, false, true, |s: &mut AppState| {
          s.add_condition()
        }),
      ),
    ),
    group(
      theme,
      (
        seg("Undo", theme, false, data.can_undo(), |s: &mut AppState| {
          s.undo()
        }),
        seg("Redo", theme, false, data.can_redo(), |s: &mut AppState| {
          s.redo()
        }),
      ),
    ),
    group(
      theme,
      seg("Recenter", theme, false, true, |s: &mut AppState| {
        s.recenter()
      }),
    ),
    group(
      theme,
      seg(format!("Palette: {}", theme.name), theme, open, true, {
        |s: &mut AppState| s.toggle_palette()
      }),
    ),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::S.px());

  // The palette row hangs off the toolbar rather than living in the panel:
  // it re-colours both panes, so it belongs to the window, not the selection.
  let picker = open.then(|| {
    let choices: Vec<_> = data
      .theme_list()
      .into_iter()
      .map(|(id, name, active)| {
        seg(name, theme, active, true, move |s: &mut AppState| {
          s.set_theme(id)
        })
        .into_any_flex()
      })
      .collect();
    flex_row((muted("Palette", theme), group(theme, choices)))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::S.px())
  });

  sized_box(
    flex_col((commands, picker))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::S.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(space::S, space::M))
  .background_color(theme.bar)
}
