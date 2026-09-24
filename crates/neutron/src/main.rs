//! Neutron — a dependency-graph task manager on the linebender stack.
//!
//! This binary wires the pure crates together behind a Xilem UI: `db` holds
//! the persistent global graph, `base` computes derived state, `layout` lays
//! the graph out, and the custom canvas widget in [`canvas`] paints it
//! (PLAN §4).

mod canvas;
mod divider;
mod field;
mod font;
mod icons;
mod state;
mod theme;
mod tokens;
mod ui;

use std::path::PathBuf;

use xilem::{EventLoop, WindowOptions, Xilem, winit::error::EventLoopError};

use crate::state::AppState;

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

  let store = match db::Store::open(&path) {
    Ok(store) => store,
    Err(e) => {
      eprintln!("neutron: cannot open {}: {e}", path.display());
      std::process::exit(1);
    }
  };
  let state = AppState::new(store);

  let app =
    Xilem::new_simple(state, ui::app_logic, WindowOptions::new("Neutron"))
      .with_font(font::DATA.to_vec())
      .with_font(icons::DATA.to_vec());
  app.run_in(EventLoop::with_user_event())
}
