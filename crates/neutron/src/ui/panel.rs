//! The side panel (PLAN §5).
//!
//! The panel is three fixed regions rather than one scrolling stack, because
//! the three things it shows change on completely different rhythms: the quest
//! lens is pinned at the top, the inspector takes the flexible middle and is
//! the only part that scrolls, and the actionable frontier is pinned at the
//! bottom where it can never be pushed out of sight. Both list regions are
//! capped, so the panel's height never grows with the graph.

use base::NodeState;
use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{
    Axis, CrossAxisAlignment, FlexExt as _, flex, flex_col, flex_row, portal,
    sized_box,
  },
};

use super::controls::{
  body, btn, chip, group, icon_btn, label, muted, row_btn, rule, section, seg,
  spacer, state_str,
};
use crate::{
  divider::{DividerAction, divider},
  field::field,
  focus::FieldKey,
  icons::{Icon, icon},
  state::{AppState, EdgeRow},
  surface::{Level, surface},
  theme::Theme,
  tokens::{radius, size, space, text},
};

/// A list of edges incident to the selection — either direction — one row per
/// edge with a button that removes it. The mark reflects whether the node at
/// the far end is satisfied.
fn edge_list(
  rows: &[EdgeRow],
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let mut items: Vec<_> = rows
    .iter()
    .map(|row| {
      let (mark, tint) = if row.satisfied {
        (Icon::Check, theme.done.1)
      } else {
        (Icon::Square, theme.muted)
      };
      let edge = row.edge;
      flex_row((
        icon(mark, text::BODY, tint),
        body(row.name.clone(), theme),
        spacer(),
        icon_btn(Icon::X, theme, false, true, move |s: &mut AppState| {
          s.remove_edge(edge)
        }),
      ))
      .must_fill_major_axis(true)
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::S.px())
      .into_any_flex()
    })
    .collect();
  if items.is_empty() {
    items.push(muted("(none)", theme).into_any_flex());
  }
  flex(Axis::Vertical, items)
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
}

/// The side card, floating over the canvas's right-hand side: lens pinned
/// top, inspector scrolling in the middle, actionable frontier pinned
/// bottom.
pub(super) fn side_card(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
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
      rule(theme),
      actionable_region(data),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(0.0.px()),
  )
  .width(data.panel_width().px())
  .expand_height();

  let divider_view = divider(theme, |s: &mut AppState, action| match action {
    DividerAction::Begin => s.begin_panel_resize(),
    DividerAction::Drag(dx) => s.resize_panel(dx),
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
  let Some(info) = data.selected_info() else {
    return flex_col((
      section("Nothing selected", theme),
      muted("Click a node on the canvas to inspect it.", theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::S.px())
    .boxed();
  };

  let done = matches!(info.state, NodeState::Completed | NodeState::Satisfied);
  let toggle_label = match (info.is_task, done) {
    (true, false) => "✓ Complete",
    (true, true) => "Reopen",
    (false, false) => "✓ Satisfy",
    (false, true) => "Unsatisfy",
  };

  // A one-line summary under the name: kind, plus quest membership when a
  // lens is active (the canvas dims pulled-in nodes, so name the distinction).
  let kind_word = if info.is_task { "Task" } else { "Condition" };
  let meta = if data.active_quest.is_some() {
    let claimed = if data.selected_is_claimed() {
      "claimed"
    } else {
      "pulled in"
    };
    format!("{kind_word} · {claimed}")
  } else {
    kind_word.to_string()
  };

  let claim_button = data.active_quest.map(|_| {
    let claimed = data.selected_is_claimed();
    let text = if claimed {
      "Unclaim"
    } else {
      "Claim for quest"
    };
    seg(text, theme, false, true, move |s: &mut AppState| {
      if claimed {
        s.unclaim_selected();
      } else {
        s.claim_selected();
      }
    })
  });

  flex_col((
    // The name field *is* the title: one place, committed as it is typed so
    // the canvas follows along and nothing is lost by clicking away.
    field(data.name_draft.clone(), theme, |s: &mut AppState, v| {
      s.rename_selected_to(v);
    })
    .size(text::TITLE)
    .focus_key(FieldKey::Title)
    .on_enter(|s: &mut AppState, _| s.finish_rename_selected()),
    flex_row((
      chip(
        state_str(info.state),
        theme.chip_for_state(info.state),
        theme,
      ),
      muted(meta, theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
    flex_row(group(
      theme,
      (
        seg(toggle_label, theme, false, true, |s: &mut AppState| {
          s.toggle_selected()
        }),
        seg("Delete", theme, false, true, |s: &mut AppState| {
          s.delete_selected()
        }),
        claim_button,
      ),
    )),
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

/// The requirement-adding control: a single button, which on arming expands
/// into the canvas prompt plus a bounded, filterable picker.
///
/// Canvas clicking is the primary path — it scales to any graph size and reads
/// off the picture in front of you. The picker is the fallback for targets
/// that are not on the canvas at all, which happens inside a quest lens, where
/// out-of-scope nodes are not drawn.
fn link_block(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  if !data.is_linking() {
    return btn("+ Add requirement", theme, |s: &mut AppState| {
      s.begin_link()
    })
    .boxed();
  }

  let (candidates, total) = data.candidate_requirements();
  let shown = candidates.len();
  let mut rows: Vec<_> = candidates
    .into_iter()
    .map(|(id, name)| {
      row_btn(
        format!("+ {name}"),
        theme,
        false,
        move |s: &mut AppState| {
          s.add_requirement(id);
          s.cancel_link();
        },
      )
      .into_any_flex()
    })
    .collect();
  if rows.is_empty() {
    rows.push(muted("No matches.", theme).into_any_flex());
  }

  let overflow = (total > shown)
    .then(|| muted(format!("{} more — type to narrow", total - shown), theme));

  sized_box(
    flex_col((
      label("Click a node on the canvas to link it.".to_string())
        .text_size(text::SECONDARY)
        .color(theme.accent),
      muted("…or search:", theme),
      field(data.link_filter.clone(), theme, |s: &mut AppState, v| {
        s.link_filter = v;
      })
      .size(text::CONTROL)
      .placeholder("Search nodes"),
      flex(Axis::Vertical, rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::XS.px()),
      overflow,
      btn("Cancel", theme, |s: &mut AppState| s.cancel_link()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::S.px()),
  )
  .padding(Padding::all(space::M))
  .background_color(theme.sunken)
  .corner_radius(radius::CARD)
  .boxed()
}

/// "What can I do right now?" (PLAN §2), pinned to the bottom so it is never
/// scrolled away by whatever the inspector happens to be showing.
fn actionable_region(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let (list, total) = data.actionable_list();
  let shown = list.len();
  let mut rows: Vec<_> = list
    .into_iter()
    .map(|(id, name)| {
      row_btn(
        format!("→ {name}"),
        theme,
        false,
        move |s: &mut AppState| s.go_to(id),
      )
      .into_any_flex()
    })
    .collect();
  if rows.is_empty() {
    rows.push(muted("Nothing ready right now.", theme).into_any_flex());
  }
  if total > shown {
    rows.push(muted(format!("+{} more", total - shown), theme).into_any_flex());
  }

  sized_box(
    flex_col((
      section(format!("Actionable · {total}"), theme),
      flex(Axis::Vertical, rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::XS.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::S.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(space::M, space::M))
}
