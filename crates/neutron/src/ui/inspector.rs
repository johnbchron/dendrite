//! The inspector card: the selected node's properties, floating down the
//! canvas's right-hand side while something is selected (PLAN §5).
//!
//! Its left edge is a drag handle that resizes it. The card does not reflow
//! the canvas when it appears, so the graph never shifts under the pointer;
//! the canvas is told the card's width instead, so fitting and revealing
//! aim at the part still visible.

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
  body, btn, chip, group, icon_btn, label, muted, row_btn, section, seg,
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
