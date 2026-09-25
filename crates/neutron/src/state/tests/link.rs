use super::*;

#[test]
fn canvas_click_links_while_armed_and_selects_otherwise() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  let frontend = node_named(&state, "Build frontend");

  // Unarmed, a click just moves the selection.
  state.select(Some(backend));
  assert!(!state.is_linking());
  state.canvas_click(Some(frontend), None, false);
  assert_eq!(state.selected, Some(frontend));

  // Armed, it builds an edge and leaves the selection put, so several
  // requirements can be added in a row.
  state.select(Some(backend));
  state.begin_link();
  assert!(state.is_linking());
  state.canvas_click(Some(frontend), None, false);
  assert_eq!(state.selected, Some(backend));
  assert!(!state.is_linking());
  let names: Vec<String> = state
    .selected_info()
    .unwrap()
    .requirements
    .into_iter()
    .map(|r| r.name)
    .collect();
  assert!(names.contains(&"Build frontend".to_string()));

  // Armed, a click on empty space cancels without linking or deselecting.
  state.begin_link();
  state.canvas_click(None, None, false);
  assert!(!state.is_linking());
  assert_eq!(state.selected, Some(backend));
}

/// Shift+click adds a requirement and stays armed; a plain click adds
/// one and disarms; empty space disarms without adding.
#[test]
fn shift_click_keeps_link_mode_armed() {
  let mut state = AppState::new(demo_store());
  let ship = node_named(&state, "Ship v1");
  let schema = node_named(&state, "Design schema");
  let signoff = node_named(&state, "Design signed off");
  state.select(Some(ship));
  state.begin_link();
  state.canvas_click(Some(schema), None, true);
  assert!(state.is_linking());
  state.canvas_click(Some(signoff), None, false);
  assert!(!state.is_linking());
  let names: Vec<_> = state
    .selected_info()
    .unwrap()
    .requirements
    .into_iter()
    .map(|r| r.name)
    .collect();
  assert!(names.contains(&"Design schema".to_string()));
  assert!(names.contains(&"Design signed off".to_string()));
  assert_eq!(state.selected, Some(ship), "linking never moves selection");
}

/// Link mode tells the canvas what a click would do.
#[test]
fn link_mode_marks_taken_and_cycle_closing_nodes() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  let schema = node_named(&state, "Design schema");
  let ship = node_named(&state, "Ship v1");
  state.select(Some(backend));
  assert!(state.link_mode().is_none(), "only while linking");
  state.begin_link();
  let mode = state.link_mode().unwrap();
  assert_eq!(mode.source, backend);
  assert_eq!(mode.name, "Build backend");
  // Itself, and what it already requires.
  assert!(mode.taken.contains(&backend) && mode.taken.contains(&schema));
  // Ship v1 requires Build backend, so Build backend requiring Ship v1
  // would close a cycle.
  assert!(mode.closes_cycle.contains(&ship));
  assert!(!mode.closes_cycle.contains(&schema));
}

#[test]
fn the_requirement_search_ranks_and_enter_takes_the_best() {
  let mut state = AppState::new(demo_store());
  let ship = node_named(&state, "Ship v1");
  let schema = node_named(&state, "Design schema");
  state.select(Some(ship));
  state.begin_link();
  state.link_filter = "sch".into();
  assert_eq!(state.candidate_requirements().0[0].0, schema);
  // Refocusing the search while armed keeps the query.
  state.begin_link();
  assert_eq!(state.link_filter, "sch");
  state.link_best_match();
  assert!(!state.is_linking());
  assert!(
    state
      .selected_info()
      .unwrap()
      .requirements
      .iter()
      .any(|r| r.other == schema)
  );
}

#[test]
fn linking_an_existing_requirement_adds_no_second_edge() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  let schema = node_named(&state, "Design schema");
  state.select(Some(backend));
  let edges_before = state.lock().graph().edges().count();
  let undoable_before = state.lock().can_undo();

  // The canvas path: arm, then click a node already required.
  state.begin_link();
  state.canvas_click(Some(schema), None, false);
  // And the direct path.
  state.add_edge(backend, schema, EdgeKind::Dependency);

  assert_eq!(state.lock().graph().edges().count(), edges_before);
  assert_eq!(state.selected_info().unwrap().requirements.len(), 1);
  // Nothing was committed, so undo still targets the seed.
  assert_eq!(state.lock().can_undo(), undoable_before);
  state.undo();
  assert_eq!(state.lock().graph().node_count(), 0);
}

#[test]
fn requirement_picker_is_bounded_and_filterable() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  for _ in 0..20 {
    state.add_task();
  }
  state.select(Some(backend));

  // However big the graph gets, the panel shows a fixed number of rows.
  let (rows, total) = state.candidate_requirements();
  assert_eq!(rows.len(), LINK_PICKER_MAX);
  assert!(total > LINK_PICKER_MAX, "expected overflow, got {total}");

  state.link_filter = "signed".into();
  let (rows, total) = state.candidate_requirements();
  assert_eq!(total, 1);
  assert_eq!(rows[0].1, "Design signed off");
}
