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
//! and which contexts are on. New places and contexts can be named from
//! there, and the library manages the rest.

use base::ContextId;
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
  body, fill, free_presets, icon_btn, label, muted, row_button, section, seg,
  spacer,
};
use crate::{
  focus::FieldKey,
  icons::{Icon, icon},
  state::{AppState, RefKind},
  theme::Theme,
  themed::{Anchor, FocusKey as _, Level, field, surface, tooltip},
  tokens::{radius, size, space, text},
};

/// Width of the tray, collapsed or open.
const WIDTH: f64 = 360.0;
/// Tallest the open list grows before it scrolls.
const MAX_LIST: f64 = 380.0;
/// Height of one row and of one group heading, for sizing the list.
const ROW: f64 = 38.0;
const HEADING: f64 = 32.0;
/// Room the context chips have in one line: the tray, less its padding,
/// the context bar's, and the tag glyph that leads the chips.
const CHIP_LINE: f64 =
  WIDTH - 2.0 * space::S - 2.0 * space::S - size::ICON as f64 - space::S;

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
  .padding(Padding::from_vh(space::CONTROL_Y, space::CONTROL_X))
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
            .padding(Padding::from_vh(space::XS, space::CONTROL_X))
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
          .padding(Padding::from_vh(space::XS, space::CONTROL_X))
          .into_any_flex(),
      );
    }
    if !soon.is_empty() {
      items.push(
        sized_box(section("Soon", theme))
          .padding(Padding::from_vh(space::XS, space::CONTROL_X))
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
    space::S,
    flex_col((header, bar, list))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::S.px()),
  ))
  .width(WIDTH.px())
}

/// The facts formula conditions read, declared here: the place (with its
/// picker), free time, and the contexts, each a chip that turns on and off.
/// A new place or context can be named here too; the library (see
/// `super::library`) is one press away for renaming and deleting them.
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
        data
          .place_name()
          .unwrap_or_else(|| "Set where you are".into()),
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
          flex_row((
            fill(body(name, theme)),
            (current == Some(id))
              .then(|| icon(Icon::Check, size::ICON, theme.accent)),
          ))
          .cross_axis_alignment(CrossAxisAlignment::Center),
          move |s: &mut AppState| s.pick_place(Some(id)),
        )
        .into_any_flex(),
      );
    }
    rows.push(
      flex_row((
        seg("+ New place", theme, false, true, |s: &mut AppState| {
          s.declare_new(RefKind::Place)
        }),
        spacer(),
        seg("Manage\u{2026}", theme, false, true, |s: &mut AppState| {
          s.toggle_library()
        }),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .into_any_flex(),
    );
    sized_box(
      flex_col(rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
    )
    .padding(Padding::left(space::L))
  });

  let free = flex_col((
    flex_row((
      icon(Icon::Hourglass, size::ICON, theme.muted),
      fill(body(
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
  .gap(space::S.px());

  let chips = data.context_chips();
  let empty = chips.is_empty();
  let mut lines = chip_lines(chips, theme);
  let add = tooltip(
    "New context, turned on",
    theme,
    Anchor::End,
    icon_btn(Icon::Plus, theme, false, true, |s: &mut AppState| {
      s.declare_new(RefKind::Context)
    }),
  );
  if empty {
    lines.push(
      sized_box(muted("No contexts yet", theme))
        .padding(Padding::from_vh(space::CONTROL_Y, 0.0))
        .into_any_flex(),
    );
  }
  let contexts = flex_row((
    icon(Icon::Tag, size::ICON, theme.muted),
    fill(
      flex_col(lines)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(space::XS.px()),
    ),
    add,
  ))
  .cross_axis_alignment(CrossAxisAlignment::Start)
  .gap(space::S.px());

  sized_box(
    flex_col((place, picker, free, contexts))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::M.px()),
  )
  .padding(Padding::all(space::S))
  .corner_radius(radius::CONTROL)
  .background_color(theme.sunken)
}

/// The context chips, broken into lines that fit the tray: xilem's flex
/// does not wrap, so a long run of contexts would run off the edge. Chip
/// widths are estimated from their names, which is close enough for a
/// line break.
fn chip_lines(
  chips: Vec<(ContextId, String, bool)>,
  theme: &'static Theme,
) -> Vec<xilem::view::AnyFlexChild<AppState>> {
  // Room kept free on the right for the add button.
  let room = CHIP_LINE - size::ICON as f64 - 2.0 * space::CONTROL_X;
  let mut lines: Vec<Vec<_>> = Vec::new();
  let mut used = f64::INFINITY;
  for (id, name, on) in chips {
    let width = name.chars().count() as f64 * f64::from(text::CONTROL) * 0.6
      + 2.0 * space::CONTROL_X
      + space::XS;
    if used + width > room {
      lines.push(Vec::new());
      used = 0.0;
    }
    used += width;
    let chip = seg(name, theme, on, true, move |s: &mut AppState| {
      s.toggle_context(id)
    });
    lines
      .last_mut()
      .expect("a line was pushed")
      .push(chip.into_any_flex());
  }
  lines
    .into_iter()
    .map(|line| {
      flex_row(line)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(space::XS.px())
        .into_any_flex()
    })
    .collect()
}
