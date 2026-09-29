//! The card for a formula condition's referent: the place, resource,
//! schedule, context or free time its atom reads, edited in place.
//!
//! Editing the referent changes every condition that points at it — rename
//! Home and every "At Home" follows — which is why the name is edited here
//! and not as the condition's own title.

use masonry::properties::types::AsUnit;
use xilem::{
  AnyWidgetView, WidgetView,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row},
};

use crate::{
  focus::FieldKey,
  icons::Icon,
  state::{AppState, FormulaInfo, RefKey, ReferentInfo},
  theme::Theme,
  themed::{Anchor, FocusKey as _, field, tooltip},
  tokens::{space, text},
  ui::controls::{body, fill, free_presets, icon_btn, muted, section, seg},
};

/// The referent's card, or nothing for an atom with nothing to edit (a
/// date) or whose referent was deleted.
pub(super) fn referent_card(
  data: &mut AppState,
  formula: &FormulaInfo,
) -> Option<Box<AnyWidgetView<AppState>>> {
  let theme = data.theme();
  let key = formula.referent.key();
  let card = match &formula.referent {
    ReferentInfo::Date | ReferentInfo::Missing => return None,
    ReferentInfo::Place { here, .. } => flex_col((
      heading("Place", key, theme),
      name_field(data, theme),
      muted(
        if *here {
          "You're here."
        } else {
          "You're somewhere else."
        },
        theme,
      ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
    .boxed(),
    ReferentInfo::Resource { balance, .. } => flex_col((
      heading("Resource", key, theme),
      name_field(data, theme),
      flex_row((
        body("Balance", theme),
        fill(
          field(
            data.balance_draft().to_string(),
            theme,
            |s: &mut AppState, v| s.set_balance_text(v),
          )
          .size(text::CONTROL)
          .focus_key(FieldKey::Balance)
          .on_enter(|s: &mut AppState, _| s.finish_balance()),
        ),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::S.px()),
      muted(format!("Have {balance}."), theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
    .boxed(),
    ReferentInfo::Schedule { spans, .. } => {
      let rows: Vec<_> = spans
        .iter()
        .enumerate()
        .map(|(i, span)| {
          flex_row((
            fill(body(span.clone(), theme)),
            icon_btn(Icon::X, theme, false, true, move |s: &mut AppState| {
              s.remove_span(i)
            }),
          ))
          .cross_axis_alignment(CrossAxisAlignment::Center)
          .into_any_flex()
        })
        .collect();
      let draft = data.span_draft().to_string();
      let hint = if spans.is_empty() && draft.is_empty() {
        Some("No windows yet, so it never opens.")
      } else if !draft.is_empty() && !data.span_draft_valid() {
        Some("Days then times, like \u{201c}weekdays 9:00-17:00\u{201d}.")
      } else {
        None
      };
      flex_col((
        heading("Schedule", key, theme),
        name_field(data, theme),
        flex_col(rows)
          .cross_axis_alignment(CrossAxisAlignment::Fill)
          .gap(space::HAIR.px()),
        field(draft, theme, |s: &mut AppState, v| s.set_span_draft(v))
          .size(text::CONTROL)
          .placeholder("Add a window: weekdays 9:00-17:00")
          .focus_key(FieldKey::Span)
          .on_enter(|s: &mut AppState, _| s.add_span()),
        hint.map(|h| muted(h, theme)),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(space::XS.px())
      .boxed()
    }
    ReferentInfo::Context { on, .. } => flex_col((
      heading("Context", key, theme),
      name_field(data, theme),
      muted(if *on { "On." } else { "Off." }, theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
    .boxed(),
    ReferentInfo::Free { until } => flex_col((
      section("Free time", theme),
      muted(until.clone().unwrap_or_else(|| "Not set.".into()), theme),
      free_presets(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
    .boxed(),
  };
  Some(card)
}

/// The referent's name, renamed as it is typed.
fn name_field(
  data: &AppState,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  field(
    data.referent_name_draft().to_string(),
    theme,
    |s: &mut AppState, v| s.rename_referent_to(v),
  )
  .size(text::CONTROL)
  .focus_key(FieldKey::ReferentName)
}

/// The card's heading, with a way to the referent in the library, where it
/// can also be deleted once nothing uses it.
fn heading(
  title: &'static str,
  key: Option<RefKey>,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  flex_row((
    fill(section(title, theme)),
    key.map(|key| {
      tooltip(
        "Rename or delete it in the library",
        theme,
        Anchor::End,
        seg(
          "Library\u{2026}",
          theme,
          false,
          true,
          move |s: &mut AppState| s.rename_referent(key),
        ),
      )
    }),
  ))
  .cross_axis_alignment(CrossAxisAlignment::Center)
  .gap(space::S.px())
}
