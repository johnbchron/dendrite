//! The Xilem view tree: a window-wide toolbar over the canvas and the side
//! panel (PLAN §5).
//!
//! `app_logic` is re-run whenever state changes; it derives the whole UI from
//! [`AppState`].
//!
//! Commands that act on the document rather than the selection — undo, redo,
//! recenter, node creation — live in the toolbar above both panes.

mod controls;
mod panel;
mod toolbar;

use masonry::properties::types::AsUnit;
use xilem::{
  WidgetView,
  view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row},
};

use crate::{
  canvas::{CanvasAction, Insets, canvas},
  divider::{DividerAction, divider},
  keymap::keymap,
  state::AppState,
};

/// Build the whole UI from the current state.
pub fn app_logic(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let theme = data.theme();
  let canvas_view = canvas(
    data.scene(),
    data.theme(),
    data.camera(),
    Insets::default(),
    |s: &mut AppState, action| match action {
      // While a link is armed this builds an edge instead of selecting.
      CanvasAction::Select(id) => s.canvas_click(id),
    },
  );

  let divider_view = divider(theme, |s: &mut AppState, action| match action {
    DividerAction::Begin => s.begin_panel_resize(),
    DividerAction::Drag(dx) => s.resize_panel(dx),
  });

  let body =
    flex_row((canvas_view.flex(1.0), divider_view, panel::side_panel(data)))
      .cross_axis_alignment(CrossAxisAlignment::Fill)
      .gap(0.0.px());

  let window = flex_col((toolbar::toolbar(data), body.flex(1.0)))
    .cross_axis_alignment(CrossAxisAlignment::Fill)
    .gap(0.0.px());
  // Outermost, so it is the window's root widget and receives every key
  // that no focused field handles.
  keymap(data.key_flags(), window, |s: &mut AppState, command| {
    s.run(command)
  })
}
