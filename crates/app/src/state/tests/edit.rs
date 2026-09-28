use super::*;

#[test]
fn removing_an_edge_undoes_cleanly() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));

  let edge = state.selected_info().unwrap().requirements[0].edge;
  state.remove_edge(edge);

  let info = state.selected_info().unwrap();
  assert!(info.requirements.is_empty());
  // The opposite direction is untouched.
  assert_eq!(info.dependents.len(), 1);

  state.undo();
  let info = state.selected_info().unwrap();
  assert_eq!(info.requirements.len(), 1);
  assert_eq!(info.requirements[0].name, "Design schema");
  // The inverse restores the original edge id, not a fresh one.
  assert_eq!(info.requirements[0].edge, edge);
}

#[test]
fn delete_and_undo_commands_round_trip() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));
  state.run(Command::Delete);
  assert!(state.lock().graph().node(backend).is_none());
  state.run(Command::Undo);
  assert!(state.lock().graph().node(backend).is_some());
  state.run(Command::Redo);
  assert!(state.lock().graph().node(backend).is_none());
}

#[test]
fn deleting_offers_undo_until_something_else_changes() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));
  state.delete_selected();
  let toast = state.toast().cloned().expect("a toast after deleting");
  assert_eq!(toast.text, "Deleted Build backend");
  state.undo_toast();
  assert!(state.lock().graph().node(backend).is_some());
  assert!(state.toast().is_none());

  // Another change makes Undo mean something else: the toast goes.
  state.select(Some(backend));
  state.delete_selected();
  state.add_task();
  assert!(state.toast().is_none());
  state.undo_toast();
  assert!(
    state.lock().graph().node(backend).is_none(),
    "the stale toast undid nothing"
  );

  // A timer for an old toast does not take down a newer one.
  let schema = node_named(&state, "Design schema");
  state.select(Some(schema));
  state.delete_selected();
  let first = state.toast().unwrap().id;
  state.undo();
  let signoff = node_named(&state, "Design signed off");
  state.select(Some(signoff));
  state.delete_selected();
  state.dismiss_toast(first);
  assert!(state.toast().is_some());
  let second = state.toast().unwrap().id;
  state.dismiss_toast(second);
  assert!(state.toast().is_none());
}

#[test]
fn a_new_node_asks_for_its_name_field() {
  let mut state = AppState::new(demo_store());
  let requests = state.focus_requests();
  assert_eq!(requests.take(), None);
  state.run(Command::New { condition: false });
  assert_eq!(requests.take(), Some(FieldKey::Title));
  assert_eq!(requests.take(), None, "served once");
}

#[test]
fn new_nodes_attach_to_the_selection_in_one_step() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));
  state.add_task();
  let new = state.selected.expect("the new task is selected");
  assert_ne!(new, backend);
  assert_eq!(state.camera().request, CameraRequest::Reveal(new));
  let reqs: Vec<_> = state
    .lock()
    .graph()
    .requirements_of(backend)
    .map(|e| e.to)
    .collect();
  assert!(
    reqs.contains(&new),
    "created as a requirement of the selection"
  );
  // One undo removes the node and its edge together.
  state.undo();
  assert!(state.lock().graph().node(new).is_none());
  assert_eq!(state.lock().graph().requirements_of(backend).count(), 1);

  // With nothing selected it stands alone.
  state.select(None);
  state.add_condition();
  let lone = state.selected.unwrap();
  assert_eq!(state.lock().graph().dependents_of(lone).count(), 0);
}

#[test]
fn renaming_commits_per_keystroke_as_one_undo_step() {
  let store = Session::new(Box::new(session::Memory::default())).unwrap();
  let mut state = AppState::new(store);
  state.add_task();
  let id = state.selected.expect("a new task is selected");
  let name = |s: &AppState| s.lock().graph().node(id).unwrap().name.clone();
  assert_eq!(state.name_draft, "New task");

  // Clearing the field to retype is held in the draft, not committed.
  state.rename_selected_to("".into());
  assert_eq!(state.name_draft, "");
  assert_eq!(name(&state), "New task");

  // Each keystroke lands in the graph, trimmed, with the draft left as
  // typed so the cursor is not disturbed.
  for text in ["R", "Re", "Ren ", "Renamed "] {
    state.rename_selected_to(text.into());
  }
  assert_eq!(name(&state), "Renamed");
  assert_eq!(state.name_draft, "Renamed ");
  state.finish_rename_selected();
  assert_eq!(state.name_draft, "Renamed");

  // Typing after Enter is a second undo step.
  state.rename_selected_to("Renamed again".into());
  state.undo();
  assert_eq!(name(&state), "Renamed");
  assert_eq!(state.name_draft, "Renamed");

  // The whole first edit undoes in one step, leaving the add intact.
  state.undo();
  assert_eq!(name(&state), "New task");
  assert_eq!(state.name_draft, "New task");
}

#[test]
fn selecting_another_node_closes_the_live_edit() {
  let store = Session::new(Box::new(session::Memory::default())).unwrap();
  let mut state = AppState::new(store);
  state.add_task();
  let a = state.selected.unwrap();
  state.add_task();
  let b = state.selected.unwrap();

  state.select(Some(a));
  state.rename_selected_to("A".into());
  state.select(Some(b));
  state.rename_selected_to("B".into());

  // Two fields, two undo steps: undoing B's rename leaves A's alone.
  state.undo();
  let graph_name = |id| state.lock().graph().node(id).unwrap().name.clone();
  assert_eq!(graph_name(a), "A");
  assert_eq!(graph_name(b), "New task");
}
