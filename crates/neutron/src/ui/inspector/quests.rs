//! The quests the selection belongs to, and adding it to others.

use base::QuestId;
use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, sized_box},
};

use super::icon_row;
use crate::{
  icons::Icon,
  state::AppState,
  themed::{Anchor, hover_row, tooltip},
  tokens::{radius, space},
  ui::controls::{body, icon_btn, muted, section},
};

/// The quests the selection belongs to, and the way to change that. Each
/// quest is a row that switches to its lens, with a remove button while
/// hovered; "Add to a quest" opens the others, and a new quest, below it.
pub(super) fn quest_list(
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
        icon_row(
          theme,
          false,
          Icon::Flag,
          if current { theme.accent } else { theme.muted },
          body(name.clone(), theme),
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
    icon_row(
      theme,
      open,
      Icon::Plus,
      theme.muted,
      muted("Add to a quest\u{2026}", theme),
      |s: &mut AppState| s.toggle_quests(),
    )
    .into_any_flex(),
  );

  let choices = open.then(|| {
    let mut choices: Vec<_> = data
      .unclaimed_quests()
      .into_iter()
      .map(|(id, name)| {
        icon_row(
          theme,
          false,
          Icon::Flag,
          theme.muted,
          body(name, theme),
          move |s: &mut AppState| s.claim_selected(id),
        )
        .into_any_flex()
      })
      .collect();
    choices.push(
      icon_row(
        theme,
        false,
        Icon::Plus,
        theme.muted,
        body("New quest with this node", theme),
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
