//! The requirement search, which arms link mode.

use app::scene::Category;
use masonry::properties::types::AsUnit;
use xilem::{
  WidgetView,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row},
};

use super::icon_row;
use crate::{
  focus::FieldKey,
  icons::Icon,
  state::AppState,
  themed::{FocusKey as _, field},
  tokens::{space, text},
  ui::controls::{body, muted},
};

/// The requirement search. Focusing it (click, or R) arms link mode, so
/// the canvas and the search work together: click a node, or type and pick a
/// match. The search is the fallback for targets that are not on the canvas,
/// which happens inside a quest lens, where out-of-scope nodes are not drawn.
///
/// Below the nodes it offers the formulas the text reads as ("at home",
/// "after oct 1", "$50"): one that exists says how many already use it, and
/// choosing it links to that node rather than making another.
pub(super) fn link_block(
  data: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let search = field(data.link_filter.clone(), theme, |s: &mut AppState, v| {
    // Typing after Enter linked a match re-arms the mode.
    s.begin_link();
    s.link_filter = v;
  })
  .size(text::CONTROL)
  .placeholder("Require\u{2026}  (R)")
  .focus_key(FieldKey::LinkSearch)
  // Escape leaves the field *and* ends link mode.
  .escape_bubbles(true)
  .on_focus(|s: &mut AppState, focused| {
    if focused {
      s.begin_link();
    }
  })
  .on_enter(|s: &mut AppState, _| s.link_best_match());

  let results = data.is_linking().then(|| {
    let (candidates, total) = data.candidate_requirements();
    let offers = data.atom_offers();
    let shown = candidates.len();
    let offers_first = candidates.is_empty();
    let mut rows: Vec<_> = candidates
      .into_iter()
      .enumerate()
      .map(|(i, (id, name))| {
        icon_row(
          theme,
          // Enter takes the first row, so it is marked.
          i == 0,
          Icon::Link,
          theme.muted,
          body(name, theme),
          move |s: &mut AppState| {
            s.add_requirement(id);
            s.cancel_link();
          },
        )
        .into_any_flex()
      })
      .collect();
    for (i, atom) in offers.into_iter().enumerate() {
      let used = (atom.used_by > 0)
        .then(|| muted(format!("\u{2014} used by {}", atom.used_by), theme));
      let glyph = Icon::for_category(Category::of(&atom.offer.atom));
      let label = atom.offer.label.clone();
      let offer = atom.offer;
      rows.push(
        icon_row(
          theme,
          // With no node to take, Enter takes the first formula.
          offers_first && i == 0,
          glyph,
          theme.muted,
          flex_row((body(label, theme), used))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(space::XS.px()),
          move |s: &mut AppState| {
            s.require_offer(offer.clone());
            s.cancel_link();
          },
        )
        .into_any_flex(),
      );
    }
    if rows.is_empty() {
      rows.push(muted("No matches.", theme).into_any_flex());
    }
    let more = (total > shown)
      .then(|| muted(format!("{} more: type to narrow", total - shown), theme));
    flex_col((
      flex_col(rows)
        .cross_axis_alignment(CrossAxisAlignment::Fill)
        .gap(space::HAIR.px()),
      more,
      muted(
        "Or click a node on the canvas. Shift+click adds several.",
        theme,
      ),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
  });

  flex_col((search, results))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(space::XS.px())
    .boxed()
}
