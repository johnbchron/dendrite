//! The Now tray: "what can I do right now?" (PLAN §2), floating at the
//! canvas's bottom-left.
//!
//! Collapsed, it is a pill with the count; open, it lists every actionable
//! node — no cap, scrolling when long — grouped by quest in the global view.
//! Choosing a row selects that node and brings it into view.

use masonry::{
  peniko::Color,
  properties::{Padding, types::AsUnit},
};
use xilem::{
  FontWeight, WidgetView,
  style::Style as _,
  view::{
    CrossAxisAlignment, FlexExt as _, button, flex_col, flex_row, portal,
    sized_box,
  },
};

use super::controls::{body, label, muted, row_button, section, spacer};
use crate::{
  icons::{Icon, icon},
  state::AppState,
  themed::{Level, surface},
  tokens::{radius, size, space, text},
};

/// Width of the tray, collapsed or open.
const WIDTH: f64 = 300.0;
/// Tallest the open list grows before it scrolls.
const MAX_LIST: f64 = 320.0;
/// Height of one row and of one group heading, for sizing the list.
const ROW: f64 = 30.0;
const HEADING: f64 = 26.0;

/// The tray, collapsed or open.
pub(super) fn tray(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let open = data.now_open();
  let (groups, total) = data.now();

  let header = button(
    flex_row((
      icon(Icon::Zap, size::ICON, theme.accent),
      label("Now")
        .text_size(text::CONTROL)
        .weight(FontWeight::SEMI_BOLD)
        .color(theme.text),
      muted(total.to_string(), theme),
      spacer(),
      icon(
        if open {
          Icon::ChevronDown
        } else {
          Icon::ChevronUp
        },
        size::ICON,
        theme.muted,
      ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px())
    .must_fill_major_axis(true),
    |s: &mut AppState| s.toggle_now(),
  )
  .padding(Padding::from_vh(space::XS, space::S))
  .corner_radius(radius::CONTROL)
  .background_color(Color::TRANSPARENT)
  .active_background_color(theme.rule)
  .border_color(Color::TRANSPARENT)
  .hovered_border_color(theme.accent);

  let list = open.then(|| {
    let headings = groups.iter().filter(|g| g.title.is_some()).count();
    let rows: usize = groups.iter().map(|g| g.items.len()).sum();
    let mut items = Vec::new();
    for group in groups {
      if let Some(title) = group.title {
        items.push(
          sized_box(section(title, theme))
            .padding(Padding::from_vh(space::XS, space::S))
            .into_any_flex(),
        );
      }
      for (id, name) in group.items {
        items.push(
          row_button(
            theme,
            false,
            body(name, theme),
            move |s: &mut AppState| s.go_to(id),
          )
          .into_any_flex(),
        );
      }
    }
    if items.is_empty() {
      items.push(
        sized_box(muted("Nothing ready right now.", theme))
          .padding(Padding::from_vh(space::XS, space::S))
          .into_any_flex(),
      );
    }
    // A portal needs a definite height: the list's own, up to the cap.
    let height =
      (rows.max(1) as f64 * ROW + headings as f64 * HEADING).min(MAX_LIST);
    sized_box(portal(
      flex_col(items)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
    ))
    .height(height.px())
  });

  sized_box(surface(
    theme,
    Level::Card,
    space::XS,
    flex_col((header, list))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::XS.px()),
  ))
  .width(WIDTH.px())
}
