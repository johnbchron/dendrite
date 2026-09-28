//! Tests for [`AppState`], by the area of the app they exercise.

mod chrome;
mod edit;
mod link;
mod now;
mod palette;
mod quests;
mod scene;
mod selection;

use std::sync::Arc;

use base::{EdgeId, EdgeKind, NodeKind, NodeState};
use layout::Slot;

use super::{
  chrome::{INSPECTOR_MAX, INSPECTOR_MIN, INSPECTOR_WIDTH},
  link::LINK_PICKER_MAX,
  palette::PaletteAct,
  selection::Primary,
  *,
};
use crate::{
  camera::CameraRequest,
  focus::FieldKey,
  keymap::{Command, Direction},
};

/// A store holding the small demo graph the app used to seed on first run:
///
/// - "Ship v1" requires "Build backend" and "Build frontend";
/// - "Build backend" requires "Design schema" (completed);
/// - "Build frontend" requires the condition "Design signed off";
/// - the quest "v1 Launch" claims "Ship v1".
pub(super) fn demo_store() -> Session {
  let mut store = Session::new(Box::new(session::Memory::default())).unwrap();
  let ship = NodeId::new();
  let backend = NodeId::new();
  let frontend = NodeId::new();
  let schema = NodeId::new();
  let signoff = NodeId::new();
  let quest = QuestId::new();

  let edge = |from, to| Event::EdgeAdded {
    edge: EdgeId::new(),
    kind: EdgeKind::Dependency,
    from,
    to,
  };
  let node = |node, name: &str, kind| Event::NodeAdded {
    node,
    kind,
    name: name.into(),
    order_hint: 0.0,
  };
  store
    .commit(vec![
      node(ship, "Ship v1", NodeKind::task()),
      node(backend, "Build backend", NodeKind::task()),
      node(frontend, "Build frontend", NodeKind::task()),
      node(schema, "Design schema", NodeKind::Task { completed: true }),
      node(signoff, "Design signed off", NodeKind::condition()),
      edge(ship, backend),
      edge(ship, frontend),
      edge(backend, schema),
      edge(frontend, signoff),
      Event::QuestCreated {
        quest,
        name: "v1 Launch".into(),
      },
      Event::QuestClaimed { quest, node: ship },
    ])
    .unwrap();
  store
}

/// Id of the seeded node with the given name.
pub(super) fn node_named(state: &AppState, name: &str) -> NodeId {
  let store = state.lock();
  let id = store
    .graph()
    .nodes()
    .find(|n| n.name == name)
    .unwrap_or_else(|| panic!("no seeded node named {name}"))
    .id;
  id
}
