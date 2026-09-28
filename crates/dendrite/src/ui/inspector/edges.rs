//! The requirement and dependent lists: one row per incident edge.

use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{Axis, CrossAxisAlignment, FlexExt as _, flex, flex_row, sized_box},
};

use crate::{
  icons::Icon,
  state::{AppState, EdgeRow},
  theme::Theme,
  themed::{Anchor, hover_row, tooltip},
  tokens::space,
  ui::controls::{body, fill, icon_btn, muted, row_button, state_dot},
};

/// A list of edges incident to the selection — either direction — one row
/// per edge. A row shows the far node's state as a dot and goes to that node
/// when pressed; its remove button appears while it is hovered.
pub(super) fn edge_list(
  rows: &[EdgeRow],
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let mut items: Vec<_> = rows
    .iter()
    .map(|row| {
      let (edge, other) = (row.edge, row.other);
      hover_row(
        row_button(
          theme,
          false,
          flex_row((
            state_dot(row.state, theme),
            fill(body(row.name.clone(), theme)),
          ))
          .cross_axis_alignment(CrossAxisAlignment::Center)
          .gap(space::S.px()),
          move |s: &mut AppState| s.go_to(other),
        ),
        tooltip(
          "Remove this link",
          theme,
          Anchor::End,
          icon_btn(Icon::X, theme, false, true, move |s: &mut AppState| {
            s.remove_edge(edge)
          }),
        ),
      )
      .into_any_flex()
    })
    .collect();
  if items.is_empty() {
    items.push(
      sized_box(muted("None", theme))
        .padding(Padding::from_vh(0.0, space::S))
        .into_any_flex(),
    );
  }
  flex(Axis::Vertical, items)
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::HAIR.px())
}
