//! The command palette: one search over nodes, commands and quests, opened
//! with Ctrl+K (or `/` for nodes only) or the search button in the top bar.
//!
//! Its search field is focused on opening; the arrow keys pass through it
//! to move the highlight, and Enter runs the highlighted row.

use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{
    CrossAxisAlignment, FlexExt as _, button, flex_col, flex_row, sized_box,
  },
};

use super::controls::{body, fill, muted, row_button, state_dot};
use crate::{
  focus::FieldKey,
  icons::{Icon, icon},
  keymap::chord,
  state::{AppState, PaletteRow, RowKind},
  theme::Theme,
  themed::{FocusKey as _, Level, field, surface},
  tokens::{radius, size, space, text},
};

/// Width of the palette.
pub(super) const WIDTH: f64 = 560.0;

/// The dimmed wash over everything behind the palette; clicking it closes
/// the palette.
pub(super) fn scrim(
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  button(sized_box(flex_col(())).expand(), |s: &mut AppState| {
    s.close_popovers()
  })
  .padding(Padding::all(0.0))
  .corner_radius(0.0)
  .background_color(theme.scrim)
  .active_background_color(theme.scrim)
  .border_width(0.0)
}

/// The palette itself.
pub(super) fn palette(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let query = data.palette_query().clone();
  let (rows, total) = data.palette_rows();
  let highlight = query.highlighted(rows.len());
  let shown = rows.len();

  let list: Vec<_> = rows
    .into_iter()
    .enumerate()
    .map(|(i, row)| palette_row(row, i == highlight, theme).into_any_flex())
    .collect();
  let empty = (shown == 0).then(|| {
    sized_box(muted("Nothing matches.", theme))
      .padding(Padding::from_vh(space::XS, space::S))
  });
  let footer = flex_row((
    muted(
      "\u{2191}\u{2193} to choose \u{b7} Enter to run \u{b7} Esc to close",
      theme,
    ),
    super::controls::spacer(),
    (total > shown)
      .then(|| muted(format!("{} more: type to narrow", total - shown), theme)),
  ))
  .must_fill_major_axis(true);

  sized_box(surface(
    theme,
    Level::Popover,
    space::S,
    flex_col((
      field(query.text, theme, |s: &mut AppState, v| {
        s.set_palette_text(v)
      })
      .placeholder(if data.palette_nodes_only() {
        "Go to a node"
      } else {
        "Search nodes, commands and quests"
      })
      .focus_key(FieldKey::PaletteSearch)
      .on_enter(|s: &mut AppState, _| s.accept_palette())
      .escape_bubbles(true),
      flex_col(list)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
      empty,
      footer,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::S.px()),
  ))
  .width(WIDTH.px())
}

/// One result: what it is (a state dot, a command or quest icon), what it
/// says, and a trailing note (a key, or the quests claiming a node).
fn palette_row(
  row: PaletteRow,
  highlighted: bool,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let act = row.act;
  let lead = match (row.kind, row.state) {
    (RowKind::Node, Some(state)) => state_dot(state, theme).boxed(),
    (RowKind::Quest, _) => icon(Icon::Flag, size::ICON, theme.muted).boxed(),
    _ => icon(Icon::Command, size::ICON, theme.muted).boxed(),
  };
  row_button(
    theme,
    highlighted,
    flex_row((
      lead,
      fill(body(row.label, theme)),
      row.detail.map(|d| muted(d, theme)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
    move |s: &mut AppState| s.run_palette(act),
  )
}

/// The top bar's way into the palette, for anyone who has not met Ctrl+K.
pub(super) fn search_button(
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  button(
    flex_row((
      icon(Icon::Search, size::ICON, theme.muted),
      muted("Search or run a command", theme),
      super::controls::spacer(),
      super::controls::label(chord("K"))
        .text_size(text::LABEL)
        .color(theme.muted),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px())
    .must_fill_major_axis(true),
    |s: &mut AppState| s.open_palette(false),
  )
  .padding(Padding::from_vh(space::XS, space::M))
  .corner_radius(radius::PILL)
  .background_color(theme.sunken)
  .active_background_color(theme.rule)
  .border_color(theme.rule)
  .hovered_border_color(theme.accent)
}
