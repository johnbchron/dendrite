use super::*;

#[test]
fn scene_reflects_seeded_graph() {
  let state = AppState::new(demo_store());
  let scene = state.scene();
  // 4 tasks + 1 condition were seeded.
  assert_eq!(scene.nodes.len(), 5);
  assert_eq!(scene.edges.len(), 4);
  // "Design schema" is done, so "Build backend" should be Ready.
  assert!(
    scene
      .nodes
      .iter()
      .any(|n| n.label == "Build backend" && n.state == NodeState::Ready)
  );
}

#[test]
fn derivations_and_scene_are_reused_until_something_changes() {
  let mut state = AppState::new(demo_store());
  let derivations = |s: &AppState| s.derivations(&s.lock());
  let (d0, s0) = (derivations(&state), state.scene());

  // Asking again, or changing nothing the scene depends on, reuses both.
  state.link_filter = "x".into();
  assert!(Arc::ptr_eq(&d0, &derivations(&state)));
  assert!(Arc::ptr_eq(&s0, &state.scene()));

  // Selection restyles the scene but leaves the graph work alone.
  state.select(Some(node_named(&state, "Build backend")));
  let s1 = state.scene();
  assert!(!Arc::ptr_eq(&s0, &s1));
  assert!(Arc::ptr_eq(&d0, &derivations(&state)));

  // A commit, and its undo, invalidate both.
  state.toggle_selected();
  let d1 = derivations(&state);
  assert!(!Arc::ptr_eq(&d0, &d1));
  assert!(!Arc::ptr_eq(&s1, &state.scene()));
  state.undo();
  assert!(!Arc::ptr_eq(&d1, &derivations(&state)));
}

/// A quest lens lays its nodes out afresh: a node heads the view when
/// nothing in the quest depends on it, whatever row it holds among every
/// node.
#[test]
fn a_lens_lays_out_its_own_rows() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  let schema = node_named(&state, "Design schema");
  let signoff = node_named(&state, "Design signed off");
  let row_of = |state: &AppState, node| {
    state
      .scene()
      .arrangement
      .rows
      .iter()
      .position(|r| r.contains(&Slot::Node(node)))
  };
  // Among every node, the sign-off sits under Build frontend, level with
  // Design schema.
  assert_eq!(row_of(&state, signoff), Some(2));
  assert_eq!(row_of(&state, schema), Some(2));

  // A quest of Build backend and the sign-off: nothing in it depends on
  // the sign-off, so it rises to the top row, beside Build backend.
  state.select(Some(backend));
  state.new_quest_with_selected();
  let quest = state.active_quest.unwrap();
  state.select(Some(signoff));
  state.claim_selected(quest);
  assert_eq!(state.scene().nodes.len(), 3);
  assert_eq!(row_of(&state, backend), Some(0));
  assert_eq!(row_of(&state, signoff), Some(0));
  assert_eq!(row_of(&state, schema), Some(1));
}
