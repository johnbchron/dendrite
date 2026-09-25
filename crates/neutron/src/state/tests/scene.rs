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
