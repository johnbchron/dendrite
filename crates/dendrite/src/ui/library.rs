//! The library popover: every place, context, resource and schedule, a
//! section per kind, hanging from the top bar.
//!
//! Pressing a row puts its name in a field, renamed as it is typed. A row
//! says what the referent holds ("Here", "$320.00") and how many conditions
//! use it; pressing that count goes to the first of them. Delete appears
//! while a row is hovered, and only does something once nothing uses the
//! referent, so no condition is left reading "Unknown place".

use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{
    CrossAxisAlignment, FlexExt as _, flex_col, flex_row, portal, sized_box,
  },
};

use super::controls::{
  body, fill, icon_btn, muted, row_button, section, seg, spacer,
};
use crate::{
  focus::FieldKey,
  icons::{Icon, icon},
  state::{AppState, LibraryItem, RefKind},
  theme::Theme,
  themed::{Anchor, FocusKey as _, Level, field, hover_row, surface, tooltip},
  tokens::{size, space, text},
};

/// Width of the popover.
const WIDTH: f64 = 380.0;
/// Tallest the list grows before it scrolls.
const MAX_LIST: f64 = 520.0;
/// Height of one row, one section heading and one hint, for sizing the
/// list.
const ROW: f64 = 38.0;
const HEADING: f64 = 40.0;
const HINT: f64 = 28.0;

/// The glyph a kind's rows lead with, as its conditions are drawn.
pub(super) fn glyph(kind: RefKind) -> Icon {
  match kind {
    RefKind::Place => Icon::MapPin,
    RefKind::Context => Icon::Tag,
    RefKind::Resource => Icon::Wallet,
    RefKind::Schedule => Icon::CalendarRange,
  }
}

/// What an empty section says, and how to fill it.
fn empty_hint(kind: RefKind) -> &'static str {
  match kind {
    RefKind::Place => "No places yet.",
    RefKind::Context => "No contexts yet.",
    RefKind::Resource => {
      "None yet. Require an amount, like \u{201c}$50\u{201d}, to add one."
    }
    RefKind::Schedule => {
      "None yet. Require \u{201c}during business hours\u{201d} to add one."
    }
  }
}

/// The popover.
pub(super) fn library(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let editing = data.library_editing();
  let draft = data.library_draft().to_string();

  let mut items = Vec::new();
  let mut height = 0.0;
  for section_data in data.library() {
    let kind = section_data.kind;
    let add = kind.addable().then(|| {
      tooltip(
        format!("New {}", kind.noun()),
        theme,
        Anchor::End,
        icon_btn(Icon::Plus, theme, false, true, move |s: &mut AppState| {
          s.new_referent(kind)
        }),
      )
    });
    items.push(
      flex_row((
        fill(section(
          format!("{} \u{b7} {}", kind.heading(), section_data.items.len()),
          theme,
        )),
        add,
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::S.px())
      .into_any_flex(),
    );
    height += HEADING;
    if section_data.items.is_empty() {
      items.push(
        sized_box(muted(empty_hint(kind), theme))
          .padding(Padding::from_vh(0.0, space::CONTROL_X))
          .into_any_flex(),
      );
      height += HINT;
    }
    for item in section_data.items {
      let row = if editing == Some(item.key) {
        editing_row(kind, draft.clone(), theme).boxed()
      } else {
        item_row(kind, item, theme).boxed()
      };
      items.push(row.into_any_flex());
      height += ROW;
    }
  }

  let list = sized_box(portal(
    flex_col(items)
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::HAIR.px()),
  ))
  .height(f64::min(height, MAX_LIST).px());

  let prune = data.prune_summary().map(|summary| {
    flex_row((
      fill(muted(format!("Unused: {summary}"), theme)),
      tooltip(
        "Remove what no condition uses, and conditions nothing requires",
        theme,
        Anchor::End,
        seg("Prune", theme, false, true, |s: &mut AppState| s.prune()),
      ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px())
  });

  sized_box(surface(
    theme,
    Level::Popover,
    space::M,
    flex_col((
      flex_row((
        body("Library", theme),
        spacer(),
        muted("Click a name to rename it", theme),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .must_fill_major_axis(true),
      list,
      prune,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::M.px()),
  ))
  .width(WIDTH.px())
}

/// A referent at rest: its glyph and name, what it holds, its uses, and
/// delete while hovered.
fn item_row(
  kind: RefKind,
  item: LibraryItem,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let key = item.key;
  let used = item.uses.len();
  let first = item.uses.first().copied();
  let uses = first.map(|node| {
    tooltip(
      "Go to the first condition that uses it",
      theme,
      Anchor::End,
      seg(
        if used == 1 {
          "1 use".to_string()
        } else {
          format!("{used} uses")
        },
        theme,
        false,
        true,
        move |s: &mut AppState| s.show_use(node),
      ),
    )
  });
  let main = flex_row((
    row_button(
      theme,
      false,
      flex_row((
        icon(glyph(kind), size::ICON, theme.muted),
        fill(body(item.name, theme)),
        item.note.map(|n| muted(n, theme)),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::S.px()),
      move |s: &mut AppState| s.rename_referent(key),
    )
    .flex(1.0),
    uses,
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::XS.px());
  let delete_tip = if used == 0 {
    format!("Delete this {}", kind.noun())
  } else {
    format!(
      "In use by {used} condition{}: change those first",
      if used == 1 { "" } else { "s" }
    )
  };
  hover_row(
    main,
    tooltip(
      delete_tip,
      theme,
      Anchor::End,
      icon_btn(
        Icon::Trash,
        theme,
        false,
        used == 0,
        move |s: &mut AppState| s.delete_referent(key),
      ),
    ),
  )
}

/// A referent whose name is being edited: the field, taking the cursor.
fn editing_row(
  kind: RefKind,
  draft: String,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  sized_box(
    flex_row((
      icon(glyph(kind), size::ICON, theme.accent),
      fill(
        field(draft, theme, |s: &mut AppState, v| s.set_library_draft(v))
          .size(text::BODY)
          .placeholder(format!("Name this {}", kind.noun()))
          .focus_key(FieldKey::LibraryName)
          .on_enter(|s: &mut AppState, _| s.finish_library_rename()),
      ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
  )
  .padding(Padding::from_vh(0.0, space::CONTROL_X))
}
