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
use masonry::{
  core::ArcStr,
  peniko::Color,
  properties::{Padding, types::AsUnit},
};
use xilem::{
  FontWeight, WidgetView,
  style::Style as _,
  view::{
    Axis, CrossAxisAlignment, FlexExt as _, FlexSequence, Label, button, flex,
    flex_col, flex_row, portal, sized_box, text_input,
  },
};

use crate::{
  canvas::{CanvasAction, canvas},
  divider::{DividerAction, divider},
  font,
  state::{AppState, EdgeRow},
  theme::Theme,
};

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

/// Every label in the panel, in the app's typeface. This shadows xilem's
/// `label` on purpose: that one sets an empty font stack, which throws away
/// even Masonry's default family (see [`font`]).
fn label(text: impl Into<ArcStr>) -> Label {
  xilem::view::label(text).font(font::STACK)
}

/// A button's label: the button scale, the palette's text colour, and muted
/// when the button is disabled (Masonry's default disabled text is a fixed
/// grey, which vanishes on a light palette).
fn btn_label<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  label(text.into())
    .text_size(SIZE_BTN)
    .color(theme.text)
    .disabled_color(theme.muted)
}

/// A standalone button, in the palette's colours.
///
/// Masonry's own button chrome is a fixed dark skin — its disabled ground is
/// pure black — so every state's ground is set here alongside the label.
fn btn<S, F>(
  text: S,
  theme: &'static Theme,
  on_press: F,
) -> impl WidgetView<AppState> + use<S, F>
where
  S: Into<String>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  button(btn_label(text, theme), on_press)
    .padding(Padding::from_vh(5.0, 12.0))
    .corner_radius(6.0)
    .background_color(theme.sunken)
    .active_background_color(theme.rule)
    .disabled_background_color(theme.bar)
    .border_color(theme.rule)
    .hovered_border_color(theme.accent)
}

/// One segment of a [`group`]: flat, so the group's frame carries the shape
/// and the segments read as one control. `active` marks the current choice
/// in a picker; `enabled` greys out a command that has nothing to do.
fn seg<S, F>(
  text: S,
  theme: &'static Theme,
  active: bool,
  enabled: bool,
  on_press: F,
) -> impl WidgetView<AppState> + use<S, F>
where
  S: Into<String>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  let ground = if active {
    theme.rule
  } else {
    Color::TRANSPARENT
  };
  button(btn_label(text, theme), on_press)
    .disabled(!enabled)
    .padding(Padding::from_vh(4.0, 10.0))
    .corner_radius(5.0)
    .background_color(ground)
    .active_background_color(theme.rule)
    .disabled_background_color(Color::TRANSPARENT)
    // A transparent border that lights up on hover, so hovering does not
    // shift the label by a pixel.
    .border_color(Color::TRANSPARENT)
    .hovered_border_color(theme.accent)
}

/// A row of [`seg`]s framed as one segmented control: related commands sit
/// together, and the toolbar reads as a few groups rather than a run of
/// identical boxes.
fn group<Seq>(
  theme: &'static Theme,
  segments: Seq,
) -> impl WidgetView<AppState> + use<Seq>
where
  Seq: FlexSequence<AppState> + Send + Sync,
{
  sized_box(
    flex_row(segments)
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(2.0.px()),
  )
  .padding(Padding::all(2.0))
  .corner_radius(7.0)
  .background_color(theme.sunken)
  .border_color(theme.rule)
  .border_width(1.0)
}

/// A list entry that acts on press — a quest to switch to, a node to jump
/// to. Left-aligned and flat, so a stack of them reads as a list rather than
/// a column of centred buttons; `active` marks the current one.
fn row_btn<S, F>(
  text: S,
  theme: &'static Theme,
  active: bool,
  on_press: F,
) -> impl WidgetView<AppState> + use<S, F>
where
  S: Into<String>,
  F: Fn(&mut AppState) + Send + Sync + 'static,
{
  let ground = if active {
    theme.sunken
  } else {
    Color::TRANSPARENT
  };
  // Filling the width inside the button is what pins the label left: the
  // button centres its child, and a full-width child has nowhere to go.
  button(sized_box(btn_label(text, theme)).expand_width(), on_press)
    .padding(Padding::from_vh(4.0, 8.0))
    .corner_radius(5.0)
    .background_color(ground)
    .active_background_color(theme.rule)
    .border_color(Color::TRANSPARENT)
    .hovered_border_color(theme.accent)
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

// --- small building blocks ----------------------------------------------

/// A 1px hairline that fills the width it is given.
fn rule(theme: &'static Theme) -> impl WidgetView<AppState> + use<> {
  sized_box(flex_col(()))
    .height(1.0.px())
    .expand_width()
    .background_color(theme.rule)
}

/// Primary panel text. Masonry's default label colour is a fixed light grey,
/// which only works on a dark ground — every palette carries its own text
/// colour instead, and this is where it goes on.
fn body<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  label(text.into()).text_size(SIZE_BODY).color(theme.text)
}

/// A section marker: a small uppercase label with a hairline running out to
/// the right edge. This replaces the per-section cards, which cost 24px of
/// horizontal room apiece without carrying any information.
fn section<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  flex_row((
    label(text.into().to_uppercase())
      .text_size(SIZE_SECTION)
      .weight(FontWeight::BOLD)
      .color(theme.muted),
    rule(theme).flex(1.0),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(8.0.px())
}

/// Muted secondary text.
fn muted<S: Into<String>>(
  text: S,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  label(text.into()).text_size(SIZE_MUTED).color(theme.muted)
}

/// A small coloured status chip.
fn chip<S: Into<String>>(
  text: S,
  color: masonry::peniko::Color,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<S> {
  sized_box(label(text.into()).text_size(SIZE_CHIP).color(theme.text))
    .padding(Padding::from_vh(2.0, 8.0))
    .background_color(color)
    .corner_radius(8.0)
}

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
      let mark = if row.satisfied { "☑" } else { "☐" };
      let edge = row.edge;
      flex_row((
        body(format!("{mark}  {}", row.name), theme).flex(1.0),
        seg("×", theme, false, true, move |s: &mut AppState| {
          s.remove_edge(edge)
        }),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(6.0.px())
      .into_any_flex()
    })
    .collect();
  if items.is_empty() {
    items.push(muted("(none)", theme).into_any_flex());
  }
  flex(Axis::Vertical, items)
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(3.0.px())
}

// --- the whole window ---------------------------------------------------

/// Build the whole UI from the current state.
pub fn app_logic(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let canvas_view = canvas(
    data.scene(),
    data.theme(),
    data.recenter_epoch(),
    |s: &mut AppState, action| match action {
      // While a link is armed this builds an edge instead of selecting.
      CanvasAction::Select(id) => s.canvas_click(id),
    },
  );

  let divider_view = divider(theme, |s: &mut AppState, action| match action {
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
  let theme = data.theme();
  let open = data.palette_open();

  let commands = flex_row((
    label("Neutron")
      .text_size(SIZE_TITLE)
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
  .gap(8.0.px());

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
      .gap(8.0.px())
  });

  sized_box(
    flex_col((commands, picker))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(7.0.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(7.0, 12.0))
  .background_color(theme.bar)
}

/// The side panel: lens pinned top, inspector scrolling in the middle,
/// actionable frontier pinned bottom.
fn side_panel(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
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
  .background_color(theme.panel)
}

// --- region 1: the quest lens -------------------------------------------

/// The active quest lens as a single always-visible line, expanding into the
/// full switcher on demand. The lens is a mode, so it needs to be readable at
/// a glance rather than inferred from an arrow in a scrollable list.
fn lens_bar(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let summary = data.active_quest_summary();
  let scoped = summary.is_some();
  let (name, claims) = match summary {
    Some((n, c)) => (n, Some(c)),
    None => ("All (global)".to_string(), None),
  };
  let open = data.picker_open();

  let head = flex_row((
    label(if scoped { "◆" } else { "○" }.to_string()).color(theme.accent),
    label(name)
      .text_size(SIZE_BODY)
      .weight(FontWeight::BOLD)
      .color(theme.text)
      .flex(1.0),
    claims.map(|c| muted(format!("{c} claimed"), theme)),
    btn(
      if open { "Close" } else { "Change" },
      theme,
      |s: &mut AppState| s.toggle_picker(),
    ),
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
        // Commits as it is typed; Enter only tidies the draft.
        text_input(data.quest_draft.clone(), |s: &mut AppState, v| {
          s.rename_active_quest_to(v);
        })
        .text_color(theme.text)
        .on_enter(|s: &mut AppState, _| s.finish_rename_quest())
        .background_color(theme.sunken)
        .border_color(theme.rule)
        .into_any_flex(),
      );
    }
    rows.push(
      row_btn("All (global)", theme, !scoped, |s: &mut AppState| {
        s.set_active_quest(None)
      })
      .into_any_flex(),
    );
    for (id, quest_name, active) in data.quest_list() {
      rows.push(
        row_btn(quest_name, theme, active, move |s: &mut AppState| {
          s.set_active_quest(Some(id))
        })
        .into_any_flex(),
      );
    }
    rows.push(
      row_btn("+ New quest", theme, false, |s: &mut AppState| {
        s.new_quest()
      })
      .into_any_flex(),
    );
    flex(Axis::Vertical, rows)
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(1.0.px())
  });

  sized_box(
    flex_col((head, picker))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(7.0.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(9.0, 12.0))
  .background_color(theme.bar)
}

// --- region 2: the inspector --------------------------------------------

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
    text_input(data.name_draft.clone(), |s: &mut AppState, v| {
      s.rename_selected_to(v);
    })
    .text_color(theme.text)
    .on_enter(|s: &mut AppState, _| s.finish_rename_selected())
    .background_color(theme.sunken)
    .border_color(theme.rule),
    flex_row((
      chip(
        state_str(info.state),
        theme.chip_for_state(info.state),
        theme,
      ),
      muted(meta, theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(8.0.px()),
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
        .text_size(SIZE_MUTED)
        .color(theme.accent),
      muted("…or search:", theme),
      text_input(data.link_filter.clone(), |s: &mut AppState, v| {
        s.link_filter = v;
      })
      .text_color(theme.text)
      .background_color(theme.sunken)
      .border_color(theme.rule),
      flex(Axis::Vertical, rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(3.0.px()),
      overflow,
      btn("Cancel", theme, |s: &mut AppState| s.cancel_link()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(6.0.px()),
  )
  .padding(Padding::all(9.0))
  .background_color(theme.sunken)
  .corner_radius(7.0)
  .boxed()
}

// --- region 3: the actionable frontier ----------------------------------

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
        move |s: &mut AppState| s.select(Some(id)),
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
        .gap(3.0.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(7.0.px()),
  )
  .expand_width()
  .padding(Padding::from_vh(10.0, 12.0))
  .background_color(theme.bar)
}
