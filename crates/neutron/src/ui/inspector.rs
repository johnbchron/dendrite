//! The inspector card: the selected node's properties, floating down the
//! canvas's right-hand side while something is selected (PLAN §5).
//!
//! Its left edge is a drag handle that resizes it. The card does not reflow
//! the canvas when it appears, so the graph never shifts under the pointer;
//! the canvas is told the card's width instead, so fitting and revealing
//! aim at the part still visible.

use base::QuestId;
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
  body, chip, fill, icon_btn, label, muted, primary_btn, row_button, section,
  spacer, state_dot, state_str,
};
use crate::{
  divider::{DividerAction, divider},
  field::field,
  focus::FieldKey,
  hover_row::hover_row,
  icons::{Icon, icon},
  state::{AppState, EdgeRow, Reason},
  surface::{Level, surface},
  theme::Theme,
  tokens::{radius, size, space, text},
  tooltip::{Anchor, tooltip},
};

/// A list of edges incident to the selection — either direction — one row
/// per edge. A row shows the far node's state as a dot and goes to that node
/// when pressed; its remove button appears while it is hovered.
fn edge_list(
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

/// Why the node is in its state: one sentence, then — when other nodes are
/// the reason — each of them as a row that goes to it.
fn reason_line(
  reason: &Reason,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let count = |n: usize, one: &str, many: &str| {
    if n == 1 {
      one.to_string()
    } else {
      format!("{n} {many}")
    }
  };
  let (sentence, alert, nodes) = match reason {
    Reason::AllMet(0) => {
      ("Nothing required: ready to do.".to_string(), false, vec![])
    }
    Reason::AllMet(n) => (
      format!(
        "{} met.",
        count(*n, "Its one requirement", "requirements, all")
      ),
      false,
      vec![],
    ),
    Reason::WaitingOn(nodes) => {
      ("Waiting on:".to_string(), false, nodes.clone())
    }
    Reason::CycleWith(nodes) => (
      "In a cycle with these; remove an edge to break it:".to_string(),
      true,
      nodes.clone(),
    ),
    Reason::Completed => ("Completed.".to_string(), false, vec![]),
    Reason::Satisfied => ("Satisfied.".to_string(), false, vec![]),
    Reason::AwaitingSatisfaction => (
      "Nothing unmet: waiting to be marked satisfied.".to_string(),
      false,
      vec![],
    ),
  };
  let rows: Vec<_> = nodes
    .into_iter()
    .map(|(id, name)| {
      row_button(
        theme,
        false,
        flex_row((
          icon(Icon::ArrowRight, size::ICON, theme.muted),
          fill(body(name, theme)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(space::S.px()),
        move |s: &mut AppState| s.go_to(id),
      )
      .into_any_flex()
    })
    .collect();
  flex_col((
    flex_row((
      alert.then(|| icon(Icon::CircleAlert, size::ICON, theme.cycle)),
      label(sentence).text_size(text::BODY).color(if alert {
        theme.cycle
      } else {
        theme.muted
      }),
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

/// The quests the selection belongs to, and the way to change that. Each
/// quest is a row that switches to its lens, with a remove button while
/// hovered; "Add to a quest" opens the others, and a new quest, below it.
fn quest_list(
  data: &mut AppState,
  claiming: &[(QuestId, String)],
) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let mut rows: Vec<_> = claiming
    .iter()
    .map(|(id, name)| {
      let id = *id;
      let current = data.active_quest == Some(id);
      hover_row(
        row_button(
          theme,
          false,
          flex_row((
            icon(
              Icon::Flag,
              size::ICON,
              if current { theme.accent } else { theme.muted },
            ),
            fill(body(name.clone(), theme)),
          ))
          .cross_axis_alignment(CrossAxisAlignment::Center)
          .gap(space::S.px()),
          move |s: &mut AppState| s.set_active_quest(Some(id)),
        ),
        tooltip(
          "Remove from this quest",
          theme,
          Anchor::End,
          icon_btn(Icon::X, theme, false, true, move |s: &mut AppState| {
            s.unclaim_selected(id)
          }),
        ),
      )
      .into_any_flex()
    })
    .collect();

  let open = data.quests_open();
  rows.push(
    row_button(
      theme,
      open,
      flex_row((
        icon(Icon::Plus, size::ICON, theme.muted),
        fill(muted("Add to a quest\u{2026}", theme)),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::S.px()),
      |s: &mut AppState| s.toggle_quests(),
    )
    .into_any_flex(),
  );

  let choices = open.then(|| {
    let mut choices: Vec<_> = data
      .unclaimed_quests()
      .into_iter()
      .map(|(id, name)| {
        row_button(
          theme,
          false,
          flex_row((
            icon(Icon::Flag, size::ICON, theme.muted),
            fill(body(name, theme)),
          ))
          .cross_axis_alignment(CrossAxisAlignment::Center)
          .gap(space::S.px()),
          move |s: &mut AppState| s.claim_selected(id),
        )
        .into_any_flex()
      })
      .collect();
    choices.push(
      row_button(
        theme,
        false,
        flex_row((
          icon(Icon::Plus, size::ICON, theme.muted),
          fill(body("New quest with this node", theme)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(space::S.px()),
        |s: &mut AppState| s.new_quest_with_selected(),
      )
      .into_any_flex(),
    );
    sized_box(
      flex_col(choices)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
    )
    .padding(Padding::all(space::XS))
    .corner_radius(radius::CONTROL)
    .background_color(theme.sunken)
  });

  flex_col((
    section(format!("Quests \u{b7} {}", claiming.len()), theme),
    flex_col(rows)
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::HAIR.px()),
    choices,
  ))
  .cross_axis_alignment(CrossAxisAlignment::Fill)
  .gap(space::S.px())
}

/// The requirement search. Focusing it (click, or R) arms link mode, so
/// the canvas and the search work together: click a node, or type and pick a
/// match. The search is the fallback for targets that are not on the canvas,
/// which happens inside a quest lens, where out-of-scope nodes are not drawn.
fn link_block(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let search = field(data.link_filter.clone(), theme, |s: &mut AppState, v| {
    // Typing after Enter linked a match re-arms the mode.
    s.begin_link();
    s.link_filter = v;
  })
  .size(text::CONTROL)
  .placeholder("Require\u{2026}  (R)")
  .focus_key(FieldKey::LinkSearch)
  // Escape leaves the field *and* ends link mode.
  .escape_bubbles(true)
  .on_focus(|s: &mut AppState, focused| {
    if focused {
      s.begin_link();
    }
  })
  .on_enter(|s: &mut AppState, _| s.link_best_match());

  let results = data.is_linking().then(|| {
    let (candidates, total) = data.candidate_requirements();
    let shown = candidates.len();
    let mut rows: Vec<_> = candidates
      .into_iter()
      .enumerate()
      .map(|(i, (id, name))| {
        row_button(
          theme,
          // Enter takes the first row, so it is marked.
          i == 0,
          flex_row((
            icon(Icon::Link, size::ICON, theme.muted),
            fill(body(name, theme)),
          ))
          .cross_axis_alignment(CrossAxisAlignment::Center)
          .gap(space::S.px()),
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
    let more = (total > shown)
      .then(|| muted(format!("{} more: type to narrow", total - shown), theme));
    flex_col((
      flex_col(rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
      more,
      muted(
        "Or click a node on the canvas. Shift+click adds several.",
        theme,
      ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
  });

  flex_col((search, results))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
    .boxed()
}
