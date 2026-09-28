//! The Now tray: "what can I do right now?" (PLAN §2), floating at the
//! canvas's bottom-left.
//!
//! Collapsed, it is a pill with the count; open, it lists every actionable
//! node — no cap, scrolling when long — grouped by quest in the global view,
//! then under Soon the tasks only the clock is holding back, soonest first.
//! Choosing a row selects that node and brings it into view.
//!
//! Open, it leads with the context bar, where the facts formula conditions
//! read are declared: where I am (C opens the picker), how long I am free,
//! and which contexts are on.

use masonry::{
  peniko::Color,
  properties::{Padding, types::AsUnit},
};
use xilem::{
  FontWeight, WidgetView,
  style::Style as _,
  view::{
    CrossAxisAlignment, FlexExt as _, button, flex_col, flex_row, portal,
    sized_box,
  },
};

use super::controls::{
  body, fill, free_presets, label, muted, row_button, section, seg, spacer,
};
use crate::{
  focus::FieldKey,
  icons::{Icon, icon},
  state::AppState,
  theme::Theme,
  themed::{FocusKey as _, Level, field, surface},
  tokens::{radius, size, space, text},
};

/// Width of the tray, collapsed or open.
const WIDTH: f64 = 300.0;
/// Tallest the open list grows before it scrolls.
const MAX_LIST: f64 = 320.0;
/// Height of one row and of one group heading, for sizing the list.
const ROW: f64 = 30.0;
const HEADING: f64 = 26.0;

/// The tray, collapsed or open.
pub(super) fn tray(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let open = data.now_open();
  let (groups, total) = data.now();
  let soon = data.soon();

  let header = button(
    flex_row((
      icon(Icon::Zap, size::ICON, theme.accent),
      label("Now")
        .text_size(text::CONTROL)
        .weight(FontWeight::SEMI_BOLD)
        .color(theme.text),
      muted(total.to_string(), theme),
      spacer(),
      icon(
        if open {
          Icon::ChevronDown
        } else {
          Icon::ChevronUp
        },
        size::ICON,
        theme.muted,
      ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px())
    .must_fill_major_axis(true),
    |s: &mut AppState| s.toggle_now(),
  )
  .padding(Padding::from_vh(space::XS, space::S))
  .corner_radius(radius::CONTROL)
  .background_color(Color::TRANSPARENT)
  .active_background_color(theme.rule)
  .border_color(Color::TRANSPARENT)
  .hovered_border_color(theme.accent);

  let list = open.then(|| {
    let headings = groups.iter().filter(|g| g.title.is_some()).count()
      + usize::from(!soon.is_empty());
    let rows: usize =
      groups.iter().map(|g| g.items.len()).sum::<usize>() + soon.len();
    let mut items = Vec::new();
    for group in groups {
      if let Some(title) = group.title {
        items.push(
          sized_box(section(title, theme))
            .padding(Padding::from_vh(space::XS, space::S))
            .into_any_flex(),
        );
      }
      for (id, name) in group.items {
        items.push(
          row_button(
            theme,
            false,
            body(name, theme),
            move |s: &mut AppState| s.go_to(id),
          )
          .into_any_flex(),
        );
      }
    }
    if items.is_empty() {
      items.push(
        sized_box(muted("Nothing ready right now.", theme))
          .padding(Padding::from_vh(space::XS, space::S))
          .into_any_flex(),
      );
    }
    if !soon.is_empty() {
      items.push(
        sized_box(section("Soon", theme))
          .padding(Padding::from_vh(space::XS, space::S))
          .into_any_flex(),
      );
    }
    for item in soon {
      let id = item.node;
      items.push(
        row_button(
          theme,
          false,
          flex_row((
            fill(body(item.name, theme)),
            muted(format!("Opens {}", item.when), theme),
          ))
          .cross_axis_alignment(CrossAxisAlignment::Center)
          .gap(space::S.px()),
          move |s: &mut AppState| s.go_to(id),
        )
        .into_any_flex(),
      );
    }
    // A portal needs a definite height: the list's own, up to the cap.
    let height =
      (rows.max(1) as f64 * ROW + headings as f64 * HEADING).min(MAX_LIST);
    sized_box(portal(
      flex_col(items)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
    ))
    .height(height.px())
  });

  let bar = open.then(|| context_bar(data, theme));

  sized_box(surface(
    theme,
    Level::Card,
    space::XS,
    flex_col((header, bar, list))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::XS.px()),
  ))
  .width(WIDTH.px())
}

/// The facts formula conditions read, declared here: the place (with its
/// picker), free time, and the contexts, each a chip that turns on and off.
fn context_bar(
  data: &mut AppState,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let place = row_button(
    theme,
    data.place_picker_open(),
    flex_row((
      icon(Icon::MapPin, size::ICON, theme.muted),
      fill(body(
        data.place_name().unwrap_or_else(|| "Set place".into()),
        theme,
      )),
      muted("C", theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
    |s: &mut AppState| s.toggle_place_picker(),
  );

  let picker = data.place_picker_open().then(|| {
    let current = data.place();
    let mut rows = vec![
      row_button(
        theme,
        current.is_none(),
        muted("Nowhere I've named", theme),
        |s: &mut AppState| s.pick_place(None),
      )
      .into_any_flex(),
    ];
    for (id, name) in data.place_choices() {
      rows.push(
        row_button(
          theme,
          current == Some(id),
          body(name, theme),
          move |s: &mut AppState| s.pick_place(Some(id)),
        )
        .into_any_flex(),
      );
    }
    flex_col(rows)
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::HAIR.px())
  });

  let free = flex_col((
    flex_row((
      icon(Icon::Hourglass, size::ICON, theme.muted),
      fill(muted(
        data
          .free_summary()
          .unwrap_or_else(|| "Free time not set".into()),
        theme,
      )),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
    field(data.free_draft.clone(), theme, |s: &mut AppState, v| {
      s.set_free_text(v)
    })
    .size(text::CONTROL)
    .placeholder("Free until 15:30, or for 1h")
    .focus_key(FieldKey::FreeUntil)
    .on_enter(|s: &mut AppState, _| s.apply_free_text()),
    free_presets(theme),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Fill)
  .gap(space::XS.px());

  let chips: Vec<_> = data
    .context_chips()
    .into_iter()
    .map(|(id, name, on)| {
      seg(name, theme, on, true, move |s: &mut AppState| {
        s.toggle_context(id)
      })
      .into_any_flex()
    })
    .collect();
  let contexts = (!chips.is_empty()).then(|| {
    flex_row((
      icon(Icon::Tag, size::ICON, theme.muted),
      flex_row(chips)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(space::XS.px()),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px())
  });

  sized_box(
    flex_col((place, picker, free, contexts))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::XS.px()),
  )
  .padding(Padding::from_vh(space::XS, space::XS))
  .corner_radius(radius::CONTROL)
  .background_color(theme.sunken)
}
