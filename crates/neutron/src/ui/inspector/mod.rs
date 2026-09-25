//! The inspector card: the selected node's properties, floating down the
//! canvas's right-hand side while something is selected (PLAN §5).
//!
//! Its left edge is a drag handle that resizes it. The card does not reflow
//! the canvas when it appears, so the graph never shifts under the pointer;
//! the canvas is told the card's width instead, so fitting and revealing
//! aim at the part still visible.

mod edges;
mod link;
mod quests;
mod reason;

use masonry::{
  peniko::Color,
  properties::{Padding, types::AsUnit},
};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{
    CrossAxisAlignment, FlexExt as _, flex_col, flex_row, portal, sized_box,
  },
};

use self::{
  edges::edge_list, link::link_block, quests::quest_list, reason::reason_line,
};
use super::controls::{
  chip, fill, icon_btn, label, muted, primary_btn, row_button, section, spacer,
  state_str,
};
use crate::{
  divider::{DividerAction, divider},
  field::field,
  focus::FieldKey,
  icons::{Icon, icon},
  state::AppState,
  surface::{Level, surface},
  theme::Theme,
  tokens::{radius, size, space, text},
  tooltip::{Anchor, tooltip},
};

/// The card, scrolling when the node's details outgrow it.
pub(super) fn card(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let regions = sized_box(
    flex_col((
      // `expand_height` makes the portal fill its flex allocation rather than
      // shrinking to its content — otherwise the pinned bottom region would
      // drift up and down with the inspector's height, which is the
      // instability this layout exists to remove.
      sized_box(portal(
        sized_box(inspector(data)).padding(Padding::all(space::L)),
      ))
      .expand_height()
      .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(0.0.px()),
  )
  .width(data.inspector_width().px())
  .expand_height();

  let divider_view = divider(theme, |s: &mut AppState, action| match action {
    DividerAction::Begin => s.begin_inspector_resize(),
    DividerAction::Drag(dx) => s.resize_inspector(dx),
  });

  // The divider is the card's left edge, so dragging it resizes the card.
  sized_box(surface(
    theme,
    Level::Card,
    0.0,
    flex_row((divider_view, regions))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(0.0.px()),
  ))
  .expand_height()
  .padding(Padding {
    top:    size::TOP_BAR + space::M,
    right:  space::M,
    bottom: space::M,
    left:   0.0,
  })
}

/// Properties of the selected node, or a hint when nothing is selected. This
/// is the only scrolling region.
fn inspector(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  // The card is only shown with a selection; this covers the frame where
  // the selected node has just been deleted.
  let Some(info) = data.selected_info() else {
    return flex_col(()).boxed();
  };

  let (kind_icon, kind_word) = if info.is_task {
    (Icon::Square, "Task")
  } else {
    (Icon::Octagon, "Condition")
  };
  // Under a lens, say whether this node is one of the quest's own or only
  // pulled in by one of them (the canvas dims the pulled-in ones).
  let membership = info.claimed.map(|claimed| {
    muted(
      if claimed {
        "· claimed"
      } else {
        "· pulled in"
      },
      theme,
    )
  });
  let header = flex_row((
    icon(kind_icon, size::ICON, theme.muted),
    muted(kind_word, theme),
    membership,
    spacer(),
    chip(
      state_str(info.state),
      theme.chip_for_state(info.state),
      theme,
    ),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::S.px())
  .must_fill_major_axis(true);

  let primary = info.primary;
  let actions = flex_row((
    primary_btn(
      primary.label(),
      theme,
      primary.enabled(),
      |s: &mut AppState| s.toggle_selected(),
    ),
    spacer(),
    tooltip(
      "More actions",
      theme,
      Anchor::End,
      icon_btn(
        Icon::Ellipsis,
        theme,
        data.more_open(),
        true,
        |s: &mut AppState| s.toggle_more(),
      ),
    ),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .must_fill_major_axis(true);

  let more = data.more_open().then(|| more_actions(theme));

  let quests = quest_list(data, &info.quests);

  flex_col((
    header,
    // The name field *is* the title: one place, committed as it is typed so
    // the canvas follows along and nothing is lost by clicking away.
    field(data.name_draft.clone(), theme, |s: &mut AppState, v| {
      s.rename_selected_to(v);
    })
    .size(text::TITLE)
    .focus_key(FieldKey::Title)
    .on_enter(|s: &mut AppState, _| s.finish_rename_selected()),
    reason_line(&info.reason, theme),
    actions,
    more,
    quests,
    section(format!("Requires · {}", info.requirements.len()), theme),
    edge_list(&info.requirements, theme),
    link_block(data),
    section(format!("Required by · {}", info.dependents.len()), theme),
    edge_list(&info.dependents, theme),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Fill)
  .gap(space::S.px())
  .boxed()
}

/// The actions behind "more": deleting (quests have their own section).
fn more_actions(theme: &'static Theme) -> impl WidgetView<AppState> + use<> {
  let delete = row_button(
    theme,
    false,
    flex_row((
      icon(Icon::Trash, size::ICON, theme.cycle),
      label("Delete").text_size(text::BODY).color(theme.cycle),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
    |s: &mut AppState| s.delete_selected(),
  );
  sized_box(
    flex_col((delete,))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::HAIR.px()),
  )
  .padding(Padding::all(space::XS))
  .corner_radius(radius::CONTROL)
  .background_color(theme.sunken)
}

/// A list row that acts on press: `glyph` in `color`, then `content` taking
/// the rest of the row. `active` marks the current (or highlighted) row.
fn icon_row<V, F>(
  theme: &'static Theme,
  active: bool,
  glyph: Icon,
  color: Color,
  content: V,
  on_press: F,
) -> impl WidgetView<AppState> + use<V, F>
where
  V: WidgetView<AppState>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  row_button(
    theme,
    active,
    flex_row((icon(glyph, size::ICON, color), fill(content)))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::S.px()),
    on_press,
  )
}
