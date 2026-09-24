//! The Xilem view tree (PLAN §5).
//!
//! `app_logic` is re-run whenever state changes; it derives the whole UI from
//! [`AppState`].
//!
//! The canvas fills the window. Everything else floats over it on a
//! `zstack`: the top bar along the top edge, the inspector card down the
//! right while something is selected, the Now tray at the bottom-left, and
//! any open popover above them, with a transparent backdrop that closes
//! the popover when clicked. The canvas is told how much of it the chrome
//! covers, so fitting and revealing aim at the part still visible.

mod controls;
mod inspector;
mod lens;
mod now;
mod toolbar;

use masonry::{
  peniko::Color,
  properties::{Padding, types::UnitPoint},
};
use xilem::{
  AnyWidgetView, WidgetView,
  style::Style as _,
  view::{button, flex_col, sized_box, zstack, zstack_item},
};

use crate::{
  canvas::{CanvasAction, canvas},
  keymap::keymap,
  state::AppState,
  tokens::{size, space},
};

/// Build the whole UI from the current state.
pub fn app_logic(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let canvas_view = canvas(
    data.scene(),
    data.theme(),
    data.camera(),
    data.canvas_insets(),
    |s: &mut AppState, action| match action {
      // While a link is armed this builds an edge instead of selecting.
      CanvasAction::Select(id) => s.canvas_click(id),
      CanvasAction::Zoomed(percent) => s.set_zoom_percent(percent),
    },
  );

  // Every layer is always present, showing nothing when it has nothing to
  // show: xilem 0.4's zstack appends a child it is asked to insert, so a
  // layer appearing mid-sequence would be paired with the wrong widget.
  let card = layer(data.selected.is_some().then(|| inspector::card(data)));
  let tray = sized_box(now::tray(data)).padding(Padding {
    top:    0.0,
    right:  0.0,
    bottom: space::M,
    left:   space::M,
  });
  let backdrop = layer(data.popover_open().then(backdrop));
  let settings = layer(data.settings_open().then(|| {
    sized_box(toolbar::settings_popover(data)).padding(Padding {
      top:    size::TOP_BAR + space::XS,
      right:  space::M,
      bottom: 0.0,
      left:   0.0,
    })
  }));
  let quests = layer(data.picker_open().then(|| {
    sized_box(lens::switcher(data)).padding(Padding {
      top:    size::TOP_BAR + space::XS,
      right:  0.0,
      bottom: 0.0,
      left:   space::M,
    })
  }));

  let window = zstack((
    canvas_view,
    zstack_item(card, UnitPoint::TOP_RIGHT),
    zstack_item(tray, UnitPoint::BOTTOM_LEFT),
    zstack_item(toolbar::top_bar(data), UnitPoint::TOP),
    zstack_item(backdrop, UnitPoint::TOP_LEFT),
    zstack_item(settings, UnitPoint::TOP_RIGHT),
    zstack_item(quests, UnitPoint::TOP_LEFT),
  ))
  .alignment(UnitPoint::TOP_LEFT);

  // Outermost, so it is the window's root widget and receives every key
  // that no focused field handles.
  keymap(data.key_flags(), window, |s: &mut AppState, command| {
    s.run(command)
  })
}

/// A zstack layer that shows `view` when there is one, and otherwise an
/// empty, zero-sized widget that neither paints nor takes the pointer.
fn layer<V>(view: Option<V>) -> Box<AnyWidgetView<AppState>>
where
  V: WidgetView<AppState>,
{
  match view {
    Some(view) => view.boxed(),
    None => flex_col(()).boxed(),
  }
}

/// A full-window, invisible button under an open popover: clicking anywhere
/// outside the popover closes it, rather than acting on what is beneath.
fn backdrop() -> impl WidgetView<AppState> + use<> {
  button(sized_box(flex_col(())).expand(), |s: &mut AppState| {
    s.close_popovers()
  })
  .padding(Padding::all(0.0))
  .background_color(Color::TRANSPARENT)
  .active_background_color(Color::TRANSPARENT)
  .border_width(0.0)
}
