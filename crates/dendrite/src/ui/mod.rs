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
mod library;
mod now;
mod palette;
mod toolbar;

use std::time::Duration;

use masonry::{
  kurbo::Vec2,
  peniko::Color,
  properties::{
    Padding,
    types::{AsUnit, UnitPoint},
  },
};
use xilem::{
  AnyWidgetView, WidgetView,
  core::fork,
  style::Style as _,
  view::{
    CrossAxisAlignment, button, flex_col, flex_row, sized_box, zstack,
    zstack_item,
  },
};

use crate::{
  canvas::{CanvasAction, LinkMode, canvas},
  font,
  icons::{Icon, icon},
  keymap::keymap,
  state::{AppState, Toast},
  theme::Theme,
  themed::{Level, Motion, after, appear, surface},
  tokens::{motion, size, space, text},
};

/// How popovers arrive: dropping a few pixels from the bar they hang off.
const DROP: Motion = Motion {
  from:        Vec2::new(0.0, -6.0),
  duration_ms: motion::POPOVER_MS,
};

/// A label in the app face (see [`crate::font`]).
fn label(text: impl Into<masonry::core::ArcStr>) -> xilem::view::Label {
  xilem::view::label(text).font(font::STACK)
}

/// How far down the window the command palette hangs.
const PALETTE_TOP: f64 = 120.0;

/// Build the whole UI from the current state.
pub fn app_logic(data: &mut AppState) -> impl WidgetView<AppState> + use<> {
  let link = data.link_mode();
  let banner = layer(link.as_ref().map(|link| link_banner(link, data.theme())));
  let canvas_view = canvas(
    data.scene(),
    data.theme(),
    data.camera(),
    data.canvas_insets(),
    link,
    |s: &mut AppState, action| match action {
      // While a link is armed this builds an edge instead of selecting.
      CanvasAction::Click { node, copy, shift } => {
        s.canvas_click(node, copy, shift);
      }
      CanvasAction::Zoomed(percent) => s.set_zoom_percent(percent),
    },
  );

  // Every layer is always present, showing nothing when it has nothing to
  // show: xilem 0.4's zstack appends a child it is asked to insert, so a
  // layer appearing mid-sequence would be paired with the wrong widget.
  let card = layer(data.selected.is_some().then(|| {
    appear(
      Motion {
        from:        Vec2::new(space::L, 0.0),
        duration_ms: motion::CARD_MS,
      },
      inspector::card(data),
    )
  }));
  let tray = sized_box(now::tray(data)).padding(Padding {
    top:    0.0,
    right:  0.0,
    bottom: space::M,
    left:   space::M,
  });
  let backdrop = layer(data.dismissable_open().then(backdrop));
  let scrim = layer(data.palette_open().then(|| palette::scrim(data.theme())));
  let palette = layer(data.palette_open().then(|| {
    sized_box(appear(DROP, palette::palette(data)))
      .padding(Padding::top(PALETTE_TOP))
  }));
  let settings = layer(data.settings_open().then(|| {
    sized_box(appear(DROP, toolbar::settings_popover(data))).padding(Padding {
      top:    size::TOP_BAR + space::XS,
      right:  space::M,
      bottom: 0.0,
      left:   0.0,
    })
  }));
  let library = layer(data.library_open().then(|| {
    sized_box(appear(DROP, library::library(data))).padding(Padding {
      top:    size::TOP_BAR + space::XS,
      right:  space::M,
      bottom: 0.0,
      left:   0.0,
    })
  }));
  let quests = layer(data.picker_open().then(|| {
    sized_box(appear(DROP, lens::switcher(data))).padding(Padding {
      top:    size::TOP_BAR + space::XS,
      right:  0.0,
      bottom: 0.0,
      left:   space::M,
    })
  }));

  let toast = layer(data.toast().cloned().map(|t| toast_view(t, data.theme())));

  let window = zstack((
    canvas_view,
    zstack_item(card, UnitPoint::TOP_RIGHT),
    zstack_item(tray, UnitPoint::BOTTOM_LEFT),
    zstack_item(toolbar::top_bar(data), UnitPoint::TOP),
    zstack_item(banner, UnitPoint::TOP),
    zstack_item(toast, UnitPoint::BOTTOM),
    zstack_item(scrim, UnitPoint::TOP_LEFT),
    zstack_item(palette, UnitPoint::TOP),
    zstack_item(backdrop, UnitPoint::TOP_LEFT),
    zstack_item(settings, UnitPoint::TOP_RIGHT),
    zstack_item(library, UnitPoint::TOP_RIGHT),
    zstack_item(quests, UnitPoint::TOP_LEFT),
  ))
  .alignment(UnitPoint::TOP_LEFT);

  // Look at the clock again when a formula could next flip, or a minute
  // on, whichever is sooner.
  let (wake, delay) = data.wake();

  // Outermost, so it is the window's root widget and receives every key
  // that no focused field handles.
  fork(
    keymap(data.key_flags(), window, |s: &mut AppState, command| {
      s.run(command)
    }),
    after(wake, delay, |s: &mut AppState| s.tick()),
  )
}

/// The banner along the canvas's top edge while link mode is armed, so the
/// mode is never invisible.
fn link_banner(
  link: &LinkMode,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  sized_box(surface(
    theme,
    Level::Popover,
    space::S,
    flex_row((
      icon(Icon::Link, size::ICON, theme.accent),
      label(format!("Pick a requirement for {}", link.name))
        .text_size(text::BODY)
        .color(theme.text),
      label("Shift+click adds several \u{b7} Esc to cancel")
        .text_size(text::SECONDARY)
        .color(theme.muted),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(space::S.px()),
  ))
  .padding(Padding {
    top:    size::TOP_BAR + space::S,
    right:  0.0,
    bottom: 0.0,
    left:   0.0,
  })
}

/// The toast: what just happened, an Undo, and a close button. It takes
/// itself down after [`motion::TOAST_MS`].
fn toast_view(
  toast: Toast,
  theme: &'static Theme,
) -> impl WidgetView<AppState> + use<> {
  let id = toast.id;
  let card = sized_box(appear(
    Motion {
      from:        Vec2::new(0.0, 8.0),
      duration_ms: motion::POPOVER_MS,
    },
    surface(
      theme,
      Level::Popover,
      space::XS,
      flex_row((
        sized_box(label(toast.text).text_size(text::BODY).color(theme.text))
          .padding(Padding::horizontal(space::S)),
        controls::seg("Undo", theme, false, true, |s: &mut AppState| {
          s.undo_toast()
        }),
        controls::icon_btn(
          Icon::X,
          theme,
          false,
          true,
          move |s: &mut AppState| s.dismiss_toast(id),
        ),
      ))
      .cross_axis_alignment(CrossAxisAlignment::Center)
      .gap(space::XS.px()),
    ),
  ))
  .padding(Padding::bottom(space::L));
  fork(
    card,
    after(
      id,
      Duration::from_millis(motion::TOAST_MS),
      move |s: &mut AppState| s.dismiss_toast(id),
    ),
  )
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
