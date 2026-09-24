//! The top bar, and the settings popover that hangs from it.
//!
//! The bar carries commands that act on the document rather than the
//! selection: creating nodes, undo and redo, the camera, settings. It is
//! the one piece of chrome that is always there.

use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, sized_box},
};

use super::controls::{
  body, fill, group, icon_btn, row_button, section, seg, spacer, swatch,
};
use crate::{
  canvas::ZoomStep,
  icons::{Icon, icon},
  keymap::chord,
  state::AppState,
  surface::{Level, surface},
  theme,
  tokens::{size, space, text},
  tooltip::{Anchor, tooltip},
};

/// The bar along the top of the window.
pub(super) fn top_bar(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();

  // With a selection, new nodes become its requirements; say so.
  let (task_tip, condition_tip) = match data.attach_point() {
    Some(name) => (
      format!("New task required by {name}"),
      format!("New condition required by {name}"),
    ),
    None => ("New task".to_string(), "New condition".to_string()),
  };
  let create = group(
    theme,
    (
      tooltip(
        task_tip,
        theme,
        Anchor::Center,
        seg("+ Task", theme, false, true, |s: &mut AppState| {
          s.add_task()
        }),
      ),
      tooltip(
        condition_tip,
        theme,
        Anchor::Center,
        seg("+ Condition", theme, false, true, |s: &mut AppState| {
          s.add_condition()
        }),
      ),
    ),
  );

  // The tooltips name the step, so it is clear what will be undone.
  let undo_tip = match data.undo_label() {
    Some(step) => format!("Undo {step} · {}", chord("Z")),
    None => "Nothing to undo".to_string(),
  };
  let redo_tip = match data.redo_label() {
    Some(step) => format!("Redo {step} · {}", chord("Shift+Z")),
    None => "Nothing to redo".to_string(),
  };
  let history = group(
    theme,
    (
      tooltip(
        undo_tip,
        theme,
        Anchor::Center,
        icon_btn(
          Icon::Undo,
          theme,
          false,
          data.can_undo(),
          |s: &mut AppState| s.undo(),
        ),
      ),
      tooltip(
        redo_tip,
        theme,
        Anchor::Center,
        icon_btn(
          Icon::Redo,
          theme,
          false,
          data.can_redo(),
          |s: &mut AppState| s.redo(),
        ),
      ),
    ),
  );

  let camera = group(
    theme,
    (
      tooltip(
        "Zoom out",
        theme,
        Anchor::Center,
        icon_btn(Icon::ZoomOut, theme, false, true, |s: &mut AppState| {
          s.zoom(ZoomStep::Out)
        }),
      ),
      // The level doubles as the reset button.
      tooltip(
        "Reset to 100%",
        theme,
        Anchor::Center,
        seg(
          format!("{}%", data.zoom_percent()),
          theme,
          false,
          true,
          |s: &mut AppState| s.zoom(ZoomStep::Reset),
        ),
      ),
      tooltip(
        "Zoom in",
        theme,
        Anchor::Center,
        icon_btn(Icon::ZoomIn, theme, false, true, |s: &mut AppState| {
          s.zoom(ZoomStep::In)
        }),
      ),
      tooltip(
        "Fit the graph",
        theme,
        Anchor::Center,
        icon_btn(Icon::Fit, theme, false, true, |s: &mut AppState| {
          s.recenter()
        }),
      ),
    ),
  );

  let settings = tooltip(
    "Settings",
    theme,
    Anchor::End,
    icon_btn(
      Icon::Settings,
      theme,
      data.settings_open(),
      true,
      |s: &mut AppState| s.toggle_settings(),
    ),
  );

  let row = flex_row((
    super::lens::pill(data),
    create,
    spacer(),
    sized_box(super::palette::search_button(theme)).width(280.0.px()),
    spacer(),
    history,
    camera,
    settings,
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::S.px());

  sized_box(surface(
    theme,
    Level::Bar,
    0.0,
    sized_box(row)
      .expand_width()
      .height(size::TOP_BAR.px())
      .padding(Padding::horizontal(space::M)),
  ))
  .expand_width()
}

/// The settings popover: for now, the palette picker. Each palette shows a
/// swatch of its ground, selection and Ready colours so it can be chosen by
/// eye.
pub(super) fn settings_popover(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
  let active = data.theme();
  let rows: Vec<_> = theme::ALL
    .iter()
    .map(|&t| {
      let chosen = t.id == active.id;
      let id = t.id;
      row_button(
        active,
        chosen,
        flex_row((
          flex_row((
            swatch(t.bg, active),
            swatch(t.accent, active),
            swatch(t.ready.1, active),
          ))
          .gap(space::XS.px()),
          fill(body(t.name, active)),
          chosen.then(|| icon(Icon::Check, text::BODY, active.accent)),
        ))
        .must_fill_major_axis(true)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(space::S.px()),
        move |s: &mut AppState| s.set_theme(id),
      )
      .into_any_flex()
    })
    .collect();

  sized_box(surface(
    active,
    Level::Popover,
    space::S,
    flex_col((
      sized_box(section("Palette", active))
        .padding(Padding::from_vh(space::XS, space::S)),
      flex_col(rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px()),
  ))
  .width(240.0.px())
}
