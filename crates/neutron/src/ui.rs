//! The Xilem view tree: a window-wide toolbar over the canvas and the side
//! panel (PLAN §5).
//!
//! `app_logic` is re-run whenever state changes; it derives the whole UI from
//! [`AppState`].
//!
//! The panel is three fixed regions rather than one scrolling stack, because
//! the three things it shows change on completely different rhythms: the quest
//! lens is pinned at the top, the inspector takes the flexible middle and is
//! the only part that scrolls, and the actionable frontier is pinned at the
//! bottom where it can never be pushed out of sight. Both list regions are
//! capped, so the panel's height never grows with the graph.
//!
//! Commands that act on the document rather than the selection — undo, redo,
//! recenter, node creation — live in the toolbar above both panes.

use base::NodeState;
use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  FontWeight, WidgetView,
  core::MessageResult,
  style::Style as _,
  view::{
    Axis, Button, CrossAxisAlignment, FlexExt as _, Label, PointerButton,
    button, flex, flex_col, flex_row, label, portal, sized_box, text_input,
  },
};

use crate::{
  canvas::{CanvasAction, canvas},
  divider::{DividerAction, divider},
  state::{AppState, EdgeRow},
};

/// Panel colour tokens (the canvas has its own palette in `canvas`).
mod palette {
  use masonry::peniko::Color;

  /// Panel and toolbar grounds. The bar sits a step above the panel so the
  /// pinned regions read as chrome rather than content.
  pub const PANEL: Color = Color::from_rgb8(18, 20, 25);
  pub const BAR: Color = Color::from_rgb8(26, 29, 36);
  /// Ground for inset blocks, such as the armed link prompt.
  pub const SUNKEN: Color = Color::from_rgb8(30, 33, 41);
  pub const RULE: Color = Color::from_rgb8(46, 50, 60);
  pub const MUTED: Color = Color::from_rgb8(122, 131, 146);
  pub const ACCENT: Color = Color::from_rgb8(94, 168, 246);

  pub const CHIP_READY: Color = Color::from_rgb8(40, 74, 118);
  pub const CHIP_DONE: Color = Color::from_rgb8(40, 82, 56);
  pub const CHIP_BLOCKED: Color = Color::from_rgb8(60, 64, 76);
  pub const CHIP_CYCLIC: Color = Color::from_rgb8(120, 54, 50);
}

/// The panel's type scale, in logical pixels.
///
/// Masonry's default label size is `theme::TEXT_SIZE_NORMAL`, which is 15 —
/// and `text_input` cannot be resized in xilem 0.4. So every size here sits
/// *below* 15, which leaves the editable node name as the largest text in the
/// panel. That is the hierarchy we want, and it is why buttons get an explicit
/// size instead of `text_button`'s default: at 15 they were bigger than every
/// heading, so the chrome outshouted the content.
const SIZE_TITLE: f32 = 14.0;
const SIZE_BODY: f32 = 13.0;
const SIZE_BTN: f32 = 12.5;
const SIZE_MUTED: f32 = 12.0;
const SIZE_CHIP: f32 = 10.5;
const SIZE_SECTION: f32 = 10.0;

/// A button whose label follows the scale above. Returns the concrete
/// [`Button`] rather than `impl WidgetView` so callers keep `.disabled()`.
fn btn<S, F>(
  text: S,
  on_press: F,
) -> Button<
  impl for<'a> Fn(&'a mut AppState, Option<PointerButton>) -> MessageResult<()>
  + Send
  + 'static,
  Label,
>
where
  S: Into<String>,
  F: Fn(&mut AppState) + Send + 'static,
{
  button(label(text.into()).text_size(SIZE_BTN), on_press)
}

/// Human-readable label for a derived node state.
fn state_str(state: NodeState) -> &'static str {
  match state {
    NodeState::Completed => "Completed",
    NodeState::Ready => "Ready",
    NodeState::Blocked => "Blocked",
    NodeState::Cyclic => "Cyclic",
    NodeState::Satisfied => "Satisfied",
    NodeState::Pending => "Pending",
  }
}

/// Background colour for a state chip.
fn state_chip_color(state: NodeState) -> masonry::peniko::Color {
  match state {
    NodeState::Completed | NodeState::Satisfied => palette::CHIP_DONE,
    NodeState::Ready => palette::CHIP_READY,
    NodeState::Cyclic => palette::CHIP_CYCLIC,
    NodeState::Blocked | NodeState::Pending => palette::CHIP_BLOCKED,
  }
}

// --- small building blocks ----------------------------------------------

/// A 1px hairline that fills the width it is given.
fn rule() -> impl WidgetView<AppState> + use<> {
  sized_box(flex_col(()))
    .height(1.0.px())
    .expand_width()
    .background_color(palette::RULE)
}

/// A section marker: a small uppercase label with a hairline running out to
/// the right edge. This replaces the per-section cards, which cost 24px of
/// horizontal room apiece without carrying any information.
fn section<S: Into<String>>(text: S) -> impl WidgetView<AppState> + use<S> {
  flex_row((
    label(text.into().to_uppercase())
      .text_size(SIZE_SECTION)
      .weight(FontWeight::BOLD)
      .color(palette::MUTED),
    rule().flex(1.0),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(8.0.px())
}

/// Muted secondary text.
fn muted<S: Into<String>>(text: S) -> impl WidgetView<AppState> + use<S> {
  label(text.into())
    .text_size(SIZE_MUTED)
    .color(palette::MUTED)
}

/// A small coloured status chip.
fn chip<S: Into<String>>(
  text: S,
  color: masonry::peniko::Color,
) -> impl WidgetView<AppState> + use<S> {
  sized_box(label(text.into()).text_size(SIZE_CHIP))
    .padding(Padding::from_vh(2.0, 8.0))
    .background_color(color)
    .corner_radius(8.0)
}

/// A list of edges incident to the selection — either direction — one row per
/// edge with a button that removes it. The mark reflects whether the node at
/// the far end is satisfied.
fn edge_list(rows: &[EdgeRow]) -> impl WidgetView<AppState> + use<> {
  let mut items: Vec<_> = rows
    .iter()
    .map(|row| {
      let mark = if row.satisfied { "☑" } else { "☐" };
      let edge = row.edge;
      flex_row((
        label(format!("{mark}  {}", row.name))
          .text_size(SIZE_BODY)
          .flex(1.0),
        btn("×", move |s: &mut AppState| s.remove_edge(edge)),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(6.0.px())
      .into_any_flex()
    })
    .collect();
  if items.is_empty() {
    items.push(muted("(none)").into_any_flex());
  }
  flex(Axis::Vertical, items)
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(3.0.px())
}

// --- the whole window ---------------------------------------------------

/// Build the whole UI from the current state.
pub fn app_logic(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let canvas_view = canvas(
    data.scene(),
    data.recenter_epoch(),
    |s: &mut AppState, action| match action {
      // While a link is armed this builds an edge instead of selecting.
      CanvasAction::Select(id) => s.canvas_click(id),
    },
  );

  let divider_view = divider(|s: &mut AppState, action| match action {
    DividerAction::Begin => s.begin_panel_resize(),
    DividerAction::Drag(dx) => s.resize_panel(dx),
  });

  let body = flex_row((canvas_view.flex(1.0), divider_view, side_panel(data)))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(0.0.px());

  flex_col((toolbar(data), body.flex(1.0)))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(0.0.px())
}

/// The slim window-wide toolbar. These commands act on the document, not on
/// the selection, so they belong above both panes rather than in the panel.
fn toolbar(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  sized_box(
    flex_row((
      label("Neutron")
        .text_size(SIZE_TITLE)
        .weight(FontWeight::BOLD)
        .flex(1.0),
      btn("Undo", |s: &mut AppState| s.undo()).disabled(!data.can_undo()),
      btn("Redo", |s: &mut AppState| s.redo()).disabled(!data.can_redo()),
      btn("Recenter", |s: &mut AppState| s.recenter()),
      btn("+ Task", |s: &mut AppState| s.add_task()),
      btn("+ Condition", |s: &mut AppState| s.add_condition()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(6.0.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(7.0, 12.0))
  .background_color(palette::BAR)
}

/// The side panel: lens pinned top, inspector scrolling in the middle,
/// actionable frontier pinned bottom.
fn side_panel(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  sized_box(
    flex_col((
      lens_bar(data),
      // `expand_height` makes the portal fill its flex allocation rather than
      // shrinking to its content — otherwise the pinned bottom region would
      // drift up and down with the inspector's height, which is the
      // instability this layout exists to remove.
      sized_box(portal(
        sized_box(inspector(data)).padding(Padding::all(12.0)),
      ))
      .expand_height()
      .flex(1.0),
      actionable_region(data),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(0.0.px()),
  )
  .width(data.panel_width().px())
  .expand_height()
  .background_color(palette::PANEL)
}

// --- region 1: the quest lens -------------------------------------------

/// The active quest lens as a single always-visible line, expanding into the
/// full switcher on demand. The lens is a mode, so it needs to be readable at
/// a glance rather than inferred from an arrow in a scrollable list.
fn lens_bar(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let summary = data.active_quest_summary();
  let scoped = summary.is_some();
  let (name, claims) = match summary {
    Some((n, c)) => (n, Some(c)),
    None => ("All (global)".to_string(), None),
  };
  let open = data.picker_open();

  let head = flex_row((
    label(if scoped { "◆" } else { "○" }.to_string()).color(palette::ACCENT),
    label(name)
      .text_size(SIZE_BODY)
      .weight(FontWeight::BOLD)
      .flex(1.0),
    claims.map(|c| muted(format!("{c} claimed"))),
    btn(if open { "Close" } else { "Change" }, |s: &mut AppState| {
      s.toggle_picker()
    }),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(8.0.px());

  let picker = open.then(|| {
    let mut rows: Vec<_> = Vec::new();
    // Renaming lives in the switcher, not in the head line: the head is a
    // status line the user reads at a glance, and an always-live field there
    // would invite stray edits to the mode they are currently working in.
    if data.active_quest.is_some() {
      rows.push(
        text_input(data.quest_draft.clone(), |s: &mut AppState, v| {
          s.quest_draft = v;
        })
        .on_enter(|s: &mut AppState, v| s.rename_active_quest_to(v))
        .into_any_flex(),
      );
    }
    rows.push(
      btn("All (global)", |s: &mut AppState| s.set_active_quest(None))
        .into_any_flex(),
    );
    for (id, quest_name, active) in data.quest_list() {
      let prefix = if active { "▸ " } else { "   " };
      rows.push(
        btn(format!("{prefix}{quest_name}"), move |s: &mut AppState| {
          s.set_active_quest(Some(id))
        })
        .into_any_flex(),
      );
    }
    rows.push(
      btn("+ New quest", |s: &mut AppState| s.new_quest()).into_any_flex(),
    );
    flex(Axis::Vertical, rows)
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(3.0.px())
  });

  sized_box(
    flex_col((head, picker))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(7.0.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(9.0, 12.0))
  .background_color(palette::BAR)
}

// --- region 2: the inspector --------------------------------------------

/// Properties of the selected node, or a hint when nothing is selected. This
/// is the only scrolling region.
fn inspector(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let Some(info) = data.selected_info() else {
    return flex_col((
      section("Nothing selected"),
      muted("Click a node on the canvas to inspect it."),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(8.0.px())
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
    btn(text, move |s: &mut AppState| {
      if claimed {
        s.unclaim_selected();
      } else {
        s.claim_selected();
      }
    })
  });

  flex_col((
    // The name field *is* the title: one place, committed on Enter.
    text_input(data.name_draft.clone(), |s: &mut AppState, v| {
      s.name_draft = v;
    })
    .on_enter(|s: &mut AppState, v| s.rename_selected_to(v)),
    flex_row((
      chip(state_str(info.state), state_chip_color(info.state)),
      muted(meta),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(8.0.px()),
    flex_row((
      btn(toggle_label, |s: &mut AppState| s.toggle_selected()),
      btn("Delete", |s: &mut AppState| s.delete_selected()),
      claim_button,
    ))
    .gap(6.0.px()),
    section(format!("Requires · {}", info.requirements.len())),
    edge_list(&info.requirements),
    link_block(data),
    section(format!("Required by · {}", info.dependents.len())),
    edge_list(&info.dependents),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Fill)
  .gap(9.0.px())
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
  if !data.is_linking() {
    return btn("+ Add requirement", |s: &mut AppState| s.begin_link()).boxed();
  }

  let (candidates, total) = data.candidate_requirements();
  let shown = candidates.len();
  let mut rows: Vec<_> = candidates
    .into_iter()
    .map(|(id, name)| {
      btn(format!("+ {name}"), move |s: &mut AppState| {
        s.add_requirement(id);
        s.cancel_link();
      })
      .into_any_flex()
    })
    .collect();
  if rows.is_empty() {
    rows.push(muted("No matches.").into_any_flex());
  }

  let overflow = (total > shown)
    .then(|| muted(format!("{} more — type to narrow", total - shown)));

  sized_box(
    flex_col((
      label("Click a node on the canvas to link it.".to_string())
        .text_size(SIZE_MUTED)
        .color(palette::ACCENT),
      muted("…or search:"),
      text_input(data.link_filter.clone(), |s: &mut AppState, v| {
        s.link_filter = v;
      }),
      flex(Axis::Vertical, rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(3.0.px()),
      overflow,
      btn("Cancel", |s: &mut AppState| s.cancel_link()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(6.0.px()),
  )
  .padding(Padding::all(9.0))
  .background_color(palette::SUNKEN)
  .corner_radius(7.0)
  .boxed()
}

// --- region 3: the actionable frontier ----------------------------------

/// "What can I do right now?" (PLAN §2), pinned to the bottom so it is never
/// scrolled away by whatever the inspector happens to be showing.
fn actionable_region(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let (list, total) = data.actionable_list();
  let shown = list.len();
  let mut rows: Vec<_> = list
    .into_iter()
    .map(|(id, name)| {
      btn(format!("→ {name}"), move |s: &mut AppState| {
        s.select(Some(id))
      })
      .into_any_flex()
    })
    .collect();
  if rows.is_empty() {
    rows.push(muted("Nothing ready right now.").into_any_flex());
  }
  if total > shown {
    rows.push(muted(format!("+{} more", total - shown)).into_any_flex());
  }

  sized_box(
    flex_col((
      section(format!("Actionable · {total}")),
      flex(Axis::Vertical, rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(3.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(7.0.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(10.0, 12.0))
  .background_color(palette::BAR)
}
