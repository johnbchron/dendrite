//! Neutron — a dependency-graph task manager on the linebender stack.
//!
//! This binary wires the pure crates together behind a Xilem UI: `db` holds
//! the persistent global graph, `base` computes derived state, `layout` lays
//! the graph out, and the custom canvas widget in [`canvas`] paints it
//! (PLAN §4).

mod canvas;
mod divider;
mod state;
mod ui;

use std::path::PathBuf;

use xilem::{EventLoop, WindowOptions, Xilem, winit::error::EventLoopError};

use crate::state::AppState;

fn main() -> Result<(), EventLoopError> {
  // One database for the whole global graph (PLAN §3). Defaults to a file in
  // the working directory; override with $NEUTRON_DB.
  let path: PathBuf = std::env::var_os("NEUTRON_DB")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from("neutron.db"));

  let store = db::Store::open(&path)
    .unwrap_or_else(|e| panic!("failed to open database at {path:?}: {e}"));
  let state = AppState::new(store);

  let app =
    Xilem::new_simple(state, ui::app_logic, WindowOptions::new("Neutron"));
  app.run_in(EventLoop::with_user_event())
}
