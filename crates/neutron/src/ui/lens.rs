//! The quest lens: a pill at the left of the top bar naming the lens in use,
//! and the switcher popover it opens.
//!
//! The lens is a mode, so it is always on screen and readable at a glance.
//! The switcher filters as you type in its search field, which it focuses
//! on opening; the arrow keys (which pass through the field) and Enter pick
//! a row, and a query no quest matches becomes the
//! name of a new quest. Renaming the active quest lives in the switcher
//! rather than on the pill, so the pill cannot be edited by accident; each
//! quest row has a delete button while hovered.

use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  FontWeight, WidgetView,
  style::Style as _,
  view::{
    CrossAxisAlignment, FlexExt as _, button, flex_col, flex_row, sized_box,
  },
};

use super::controls::{
  body, fill, icon_btn, label, muted, row_button, section,
};
use crate::{
  field::field,
  focus::FieldKey,
  hover_row::hover_row,
  icons::{Icon, icon},
  state::{AppState, QuestChoice},
  surface::{Level, surface},
  tokens::{radius, size, space, text},
  tooltip::{Anchor, tooltip},
};

/// Width of the switcher popover.
const WIDTH: f64 = 300.0;

/// The pill: the lens's name and claim count, opening the switcher.
pub(super) fn pill(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let open = data.picker_open();
  let summary = data.active_quest_summary();
  let scoped = summary.is_some();
  let (name, claims) = match summary {
    Some((name, claims)) => (name, Some(claims)),
    None => ("All nodes".to_string(), None),
  };

  let content = flex_row((
    icon(
      Icon::Flag,
      size::ICON,
      if scoped { theme.accent } else { theme.muted },
    ),
    label(name)
      .text_size(text::CONTROL)
      .weight(FontWeight::SEMI_BOLD)
      .color(theme.text),
    claims.map(|c| muted(format!("{c} claimed"), theme)),
    icon(Icon::ChevronDown, size::ICON, theme.muted),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::S.px());

  button(content, |s: &mut AppState| s.toggle_picker())
    .padding(Padding::from_vh(space::XS, space::M))
    .corner_radius(radius::PILL)
    .background_color(theme.sunken)
    .active_background_color(theme.rule)
    .border_color(if open { theme.accent } else { theme.rule })
    .hovered_border_color(theme.accent)
}

/// The switcher popover.
pub(super) fn switcher(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let query = data.quest_query().clone();
  let rows = data.quest_rows();
  let highlight = query.highlighted(rows.len());

  let rename = data.active_quest.is_some().then(|| {
    flex_col((
      sized_box(section("Name", theme))
        .padding(Padding::from_vh(0.0, space::XS)),
      // Commits as it is typed; Enter only tidies the draft.
      field(data.quest_draft.clone(), theme, |s: &mut AppState, v| {
        s.rename_active_quest_to(v);
      })
      .size(text::BODY)
      .focus_key(FieldKey::QuestName)
      .on_enter(|s: &mut AppState, _| s.finish_rename_quest()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
  });

  let list: Vec<_> = rows
    .into_iter()
    .enumerate()
    .map(|(i, row)| {
      let choice = row.choice;
      let lead = match choice {
        QuestChoice::New => icon(Icon::Plus, size::ICON, theme.muted),
        _ if row.current => icon(Icon::Flag, size::ICON, theme.accent),
        _ => icon(Icon::Flag, size::ICON, theme.muted),
      };
      let button = row_button(
        theme,
        i == highlight,
        flex_row((
          lead,
          fill(body(row.label, theme)),
          row
            .current
            .then(|| icon(Icon::Check, size::ICON, theme.accent)),
        ))
        .must_fill_major_axis(true)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(space::S.px()),
        move |s: &mut AppState| s.choose_quest(choice),
      );
      match choice {
        QuestChoice::Quest(id) => hover_row(
          button,
          tooltip(
            "Delete quest",
            theme,
            Anchor::End,
            icon_btn(
              Icon::Trash,
              theme,
              false,
              true,
              move |s: &mut AppState| s.delete_quest(id),
            ),
          ),
        )
        .into_any_flex(),
        _ => button.into_any_flex(),
      }
    })
    .collect();

  sized_box(surface(
    theme,
    Level::Popover,
    space::S,
    flex_col((
      field(query.text, theme, |s: &mut AppState, v| s.set_quest_text(v))
        .placeholder("Find or create a quest")
        .focus_key(FieldKey::QuestSearch)
        .on_enter(|s: &mut AppState, _| s.accept_quest())
        // Escape leaves the field and closes the switcher in one press.
        .escape_bubbles(true),
      rename,
      flex_col(list)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
      muted("↑↓ to choose · Enter to switch · Esc to close", theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::S.px()),
  ))
  .width(WIDTH.px())
}
