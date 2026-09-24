//! The Xilem view tree (PLAN §5).
//!
//! `app_logic` is re-run whenever state changes; it derives the whole UI from
//! [`AppState`].
//!
//! The canvas fills the window. Everything else floats over it on a
//! `zstack`: the top bar along the top edge, the side card down the right,
//! and any open popover above them, with a transparent backdrop that closes
//! the popover when clicked. The canvas is told how much of it the chrome
//! covers, so fitting and revealing aim at the part still visible.

mod controls;
mod panel;
mod toolbar;

use masonry::{
  peniko::Color,
  properties::{Padding, types::UnitPoint},
};
use xilem::{
  WidgetView,
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

  let backdrop = data.popover_open().then(backdrop);
  let settings = data.settings_open().then(|| {
    zstack_item(
      sized_box(toolbar::settings_popover(data)).padding(Padding {
        top:    size::TOP_BAR + space::XS,
        right:  space::M,
        bottom: 0.0,
        left:   0.0,
      }),
      UnitPoint::TOP_RIGHT,
    )
  });

  let window = zstack((
    canvas_view,
    zstack_item(panel::side_card(data), UnitPoint::TOP_RIGHT),
    zstack_item(toolbar::top_bar(data), UnitPoint::TOP),
    backdrop,
    settings,
  ))
  .alignment(UnitPoint::TOP_LEFT);

  // Outermost, so it is the window's root widget and receives every key
  // that no focused field handles.
  keymap(data.key_flags(), window, |s: &mut AppState, command| {
    s.run(command)
  })
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
