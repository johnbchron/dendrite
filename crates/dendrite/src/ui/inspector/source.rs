//! "Satisfied by": what makes the selected condition hold — a click, or a
//! formula over facts — chosen in the inspector, and the small form for
//! each kind of formula.
//!
//! Applying the form turns the condition into the chosen kind in place:
//! everything that required it still does.

use app::{formula::phrase::Offer, scene::Category};
use masonry::properties::{Padding, types::AsUnit};
use xilem::{
  AnyWidgetView, WidgetView,
  style::Style as _,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, sized_box},
};

use super::icon_row;
use crate::{
  focus::FieldKey,
  icons::Icon,
  state::{AppState, SourceDraft, SourceKind},
  theme::Theme,
  themed::{FocusKey as _, field},
  tokens::{radius, space, text},
  ui::controls::{body, label, muted, primary_btn, row_button, section, seg},
};

/// The choice of what satisfies the selected condition, and the open form;
/// nothing for a task.
pub(super) fn source_block(
  data: &mut AppState,
) -> Option<Box<AnyWidgetView<AppState>>> {
  let current = data.source_kind()?;
  let theme = data.theme();
  let draft = data.source_draft().cloned();
  let chosen = draft.as_ref().map_or(current, |d| d.kind);

  let choice = |kinds: &[SourceKind]| {
    let segs: Vec<_> = kinds
      .iter()
      .map(|&kind| {
        seg(
          kind.label(),
          theme,
          kind == chosen,
          true,
          move |s: &mut AppState| s.choose_source(kind),
        )
        .into_any_flex()
      })
      .collect();
    flex_row(segs)
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::HAIR.px())
  };

  Some(
    flex_col((
      section("Satisfied by", theme),
      choice(&SourceKind::ALL[..4]),
      choice(&SourceKind::ALL[4..]),
      draft.map(|draft| form(data, draft, theme)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
    .boxed(),
  )
}

/// The form for the chosen kind: its fields, what applying it makes, and
/// Apply and Cancel.
fn form(
  data: &mut AppState,
  draft: SourceDraft,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let before = (draft.kind == SourceKind::Date).then(|| {
    flex_row((
      seg("After", theme, !draft.before, true, |s: &mut AppState| {
        s.set_source_before(false)
      }),
      seg("Before", theme, draft.before, true, |s: &mut AppState| {
        s.set_source_before(true)
      }),
    ))
    .gap(space::HAIR.px())
  });

  let choices: Vec<_> = data
    .source_choices()
    .into_iter()
    .map(|(raw, name)| {
      row_button(
        theme,
        draft.pick == Some(raw),
        body(name, theme),
        move |s: &mut AppState| s.pick_source(raw),
      )
      .into_any_flex()
    })
    .collect();
  let none_yet = draft.kind.picks() && choices.is_empty();

  let placeholder = match draft.kind {
    SourceKind::Manual => "Its name",
    SourceKind::Date => "oct 1, friday 9am, 2026-10-01",
    SourceKind::FreeTime => "How long: 30m, 1h 30m",
    SourceKind::Money if none_yet => "An amount: $50, 3 batteries",
    SourceKind::Money => "How much: 50, 12.50",
    SourceKind::Place => "Or a new place's name",
    SourceKind::Schedule => "Or a new schedule's name",
    SourceKind::Context => "Or a new context's name",
  };
  let input = field(draft.text.clone(), theme, |s: &mut AppState, v| {
    s.set_source_text(v)
  })
  .size(text::CONTROL)
  .placeholder(placeholder)
  .focus_key(FieldKey::Source)
  .on_enter(|s: &mut AppState, _| s.apply_source());

  let target = data.source_target();
  let outcome = match &target {
    Ok(target) => muted(format!("Becomes {}.", target.label()), theme),
    Err(need) => muted(need.to_string(), theme),
  };
  let drops = data.source_drops();
  let warning = (drops > 0).then(|| {
    label(format!(
      "Its {} will be removed: a formula condition has none.",
      if drops == 1 {
        "requirement".to_string()
      } else {
        format!("{drops} requirements")
      }
    ))
    .text_size(text::SECONDARY)
    .color(theme.cycle)
  });

  let buttons = flex_row((
    primary_btn("Apply", theme, target.is_ok(), |s: &mut AppState| {
      s.apply_source()
    }),
    seg("Cancel", theme, false, true, |s: &mut AppState| {
      s.cancel_source()
    }),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::S.px());

  sized_box(
    flex_col((
      before,
      flex_col(choices)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
      input,
      outcome,
      warning,
      buttons,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px()),
  )
  .padding(Padding::all(space::S))
  .corner_radius(radius::CONTROL)
  .background_color(theme.sunken)
}

/// Under a manual condition's name: the formulas the name reads as, to
/// turn it into with a click. The one Enter would choose is marked.
pub(super) fn title_offers(
  data: &mut AppState,
) -> Option<impl WidgetView<AppState> + use<>> {
  let offers = data.title_offers();
  if offers.is_empty() {
    return None;
  }
  let theme = data.theme();
  let reading = data.title_reading().map(|o| o.atom);
  let rows: Vec<_> = offers
    .into_iter()
    .take(4)
    .map(|offer: Offer| {
      let glyph = Icon::for_category(Category::of(&offer.atom));
      let marked = reading.as_ref() == Some(&offer.atom);
      let text = format!("Make it {}", offer.label);
      icon_row(
        theme,
        marked,
        glyph,
        theme.muted,
        body(text, theme),
        move |s| s.make_automatic(offer.clone()),
      )
      .into_any_flex()
    })
    .collect();
  Some(
    flex_col(rows)
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::HAIR.px()),
  )
}
