//! Neutron — a dependency-graph task manager on the linebender stack.
//!
//! This binary wires the pure crates together behind a Xilem UI: `db` holds
//! the persistent global graph, `base` computes derived state, `layout` lays
//! the graph out, and the custom canvas widget in [`canvas`] paints it
//! (PLAN §4).

mod appear;
mod canvas;
mod divider;
mod driver;
mod field;
mod focus;
mod font;
mod hover_row;
mod icons;
mod keymap;
mod query;
mod state;
mod surface;
mod theme;
mod timer;
mod tokens;
mod tooltip;
mod ui;

use std::path::PathBuf;

use xilem::{
  EventLoop, WindowOptions, Xilem, masonry::theme::default_property_set,
  winit::error::EventLoopError,
};

use crate::{driver::FocusDriver, state::AppState};

fn main() -> Result<(), EventLoopError> {
  // One database for the whole global graph (PLAN §3). Defaults to a file in
  // the working directory; override with $NEUTRON_DB.
  let path: PathBuf = std::env::var_os("NEUTRON_DB")
    .map(PathBuf::from)
    .or(dirs::data_dir().map(|pb| {
      let path = pb.join("neutron");
      std::fs::create_dir_all(&path).expect("failed to create database dir");
      path.join("neutron.db")
    }))
    .unwrap_or_else(|| PathBuf::from("neutron.db"));

  let store = match db::open(&path) {
    Ok(store) => store,
    Err(e) => {
      eprintln!("neutron: cannot open {}: {e}", path.display());
      std::process::exit(1);
    }
  };
  let state = AppState::new(store);
  let focus_requests = state.focus_requests();

  let app =
    Xilem::new_simple(state, ui::app_logic, WindowOptions::new("Neutron"))
      .with_font(font::DATA.to_vec())
      .with_font(icons::DATA.to_vec());

  // What `Xilem::run_in` does, with xilem's driver wrapped so the app can
  // focus fields created by an action (see `driver`).
  let event_loop = EventLoop::with_user_event().build()?;
  let proxy = event_loop.create_proxy();
  let (xilem_driver, windows) = app.into_driver_and_windows(move |event| {
    proxy.send_event(event).map_err(|err| err.0)
  });
  masonry_winit::app::run_with(
    event_loop,
    windows,
    FocusDriver::new(xilem_driver, focus_requests),
    default_property_set(),
  )
}
