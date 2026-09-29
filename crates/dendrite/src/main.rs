//! Dendrite — a dependency-graph task manager on the linebender stack.
//!
//! This binary is the Xilem UI and nothing else. `app` decides what the
//! program does, `db` keeps it, `base` computes derived state and `layout`
//! lays the graph out; here we draw the result and turn input back into
//! calls on [`app::state::AppState`] (PLAN §4).
//!
//! The modules `app` owns are re-exported below under the paths the views
//! already use, so `crate::theme`, `crate::state` and the rest still name
//! the one definition. The generic widgets live in `widgets`; [`themed`]
//! binds them to this app's palette and tokens.

mod canvas;
mod driver;
mod focus;
mod font;
mod icons;
mod keymap;
mod themed;
mod ui;

use std::path::PathBuf;

pub use app::{query, state, theme, tokens};
use xilem::{
  EventLoop, WindowOptions, Xilem, masonry::theme::default_property_set,
  winit::error::EventLoopError,
};

use crate::{driver::FocusDriver, state::AppState};

/// The application ID: the Wayland app_id and X11 `WM_CLASS`, which desktops
/// match against the installed `packaging/linux/sh.jlewis.Dendrite.desktop`.
#[cfg(target_os = "linux")]
const APP_ID: &str = "sh.jlewis.Dendrite";

fn main() -> Result<(), EventLoopError> {
  // One database for the whole global graph (PLAN §3). Defaults to a file in
  // the working directory; override with $DENDRITE_DB.
  let path: PathBuf = std::env::var_os("DENDRITE_DB")
    .map(PathBuf::from)
    .or(dirs::data_dir().map(|pb| {
      let path = pb.join("dendrite");
      std::fs::create_dir_all(&path).expect("failed to create database dir");
      path.join("dendrite.db")
    }))
    .unwrap_or_else(|| PathBuf::from("dendrite.db"));

  let store = match db::open(&path) {
    Ok(store) => store,
    Err(e) => {
      eprintln!("dendrite: cannot open {}: {e}", path.display());
      std::process::exit(1);
    }
  };
  let state = AppState::new(store);
  let focus_requests = state.focus_requests();

  let app =
    Xilem::new_simple(state, ui::app_logic, WindowOptions::new("Dendrite"))
      .with_font(font::DATA.to_vec())
      .with_font(icons::DATA.to_vec());

  // What `Xilem::run_in` does, with xilem's driver wrapped so the app can
  // focus fields created by an action (see `driver`).
  let event_loop = EventLoop::with_user_event().build()?;
  let proxy = event_loop.create_proxy();
  let (xilem_driver, windows) = app.into_driver_and_windows(move |event| {
    proxy.send_event(event).map_err(|err| err.0)
  });
  // `WindowOptions` has no way to name the application on Linux, so it is
  // set on the attributes it produced. The Wayland and X11 extensions share
  // one setting; this sets both.
  #[cfg(target_os = "linux")]
  let windows: Vec<_> = windows
    .into_iter()
    .map(|mut window| {
      use xilem::winit::platform::wayland::WindowAttributesExtWayland;
      window.attributes = window.attributes.with_name(APP_ID, "dendrite");
      window
    })
    .collect();
  masonry_winit::app::run_with(
    event_loop,
    windows,
    FocusDriver::new(xilem_driver, focus_requests),
    default_property_set(),
  )
}
