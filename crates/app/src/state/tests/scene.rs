use base::Atom;

use super::*;

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

/// A condition required from two rows of one tree is drawn twice, one copy
/// under each dependent; the copies share the node's state, and the edges
/// into them are marked.
#[test]
fn a_shared_condition_is_drawn_once_per_row_of_dependents() {
  let mut state = AppState::new(demo_store());
  let ship = node_named(&state, "Ship v1");
  let frontend = node_named(&state, "Build frontend");
  let signoff = node_named(&state, "Design signed off");
  state.add_edge(ship, signoff, EdgeKind::Dependency);

  let scene = state.scene();
  let copies: Vec<_> =
    scene.nodes.iter().filter(|n| n.node == signoff).collect();
  assert_eq!(copies.len(), 2);
  assert!(copies.iter().all(|n| n.copies == 2));
  assert!(
    copies.iter().any(|n| n.id == signoff),
    "the first keeps its id"
  );
  let into: Vec<_> = scene
    .edges
    .iter()
    .filter(|e| e.from == ship || e.from == frontend)
    .filter(|e| e.to_copy)
    .collect();
  assert_eq!(into.len(), 2);
  assert_ne!(into[0].to, into[1].to, "each dependent has its own copy");
  assert!(
    scene
      .edges
      .iter()
      .filter(|e| !e.to_copy)
      .all(|e| !copies.iter().any(|c| c.id == e.to))
  );

  // Satisfying the condition satisfies every copy.
  state.select(Some(signoff));
  state.toggle_selected();
  let scene = state.scene();
  assert!(
    scene
      .nodes
      .iter()
      .filter(|n| n.node == signoff)
      .all(|n| n.state == NodeState::Satisfied && n.selected)
  );
}

/// The arrow keys step between boxes: down from a dependent to the copy
/// that serves it, and back up from that copy to the same dependent, also
/// when the copy was clicked.
#[test]
fn navigation_moves_through_the_copies_as_drawn() {
  let mut state = AppState::new(demo_store());
  let ship = node_named(&state, "Ship v1");
  let frontend = node_named(&state, "Build frontend");
  let signoff = node_named(&state, "Design signed off");
  state.add_edge(ship, signoff, EdgeKind::Dependency);
  let copy_for = |state: &AppState, dependent| {
    state
      .scene()
      .edges
      .iter()
      .find(|e| e.from == dependent && e.to_copy)
      .unwrap()
      .to
  };
  let under_frontend = copy_for(&state, frontend);
  let under_ship = copy_for(&state, ship);

  state.select(Some(frontend));
  state.run(Command::Nav(Direction::Down));
  assert_eq!(state.selected, Some(signoff));
  assert_eq!(state.selected_copy, Some(under_frontend));
  state.run(Command::Nav(Direction::Up));
  assert_eq!(state.selected, Some(frontend));

  // Clicking the other copy and going up reaches its own dependent.
  state.canvas_click(Some(signoff), Some(under_ship), false);
  state.run(Command::Nav(Direction::Up));
  assert_eq!(state.selected, Some(ship));
}

/// A formula condition is never marked, however it is reached, but still
/// shows in the lens of a quest whose work requires it.
#[test]
fn a_formula_condition_has_no_quest_bar() {
  let mut state = AppState::new(demo_store());
  let signoff = node_named(&state, "Design signed off");
  state.select(Some(signoff));
  state.choose_source(SourceKind::FreeTime);
  state.set_source_text("1h".into());
  state.apply_source();
  let free = Atom::Free { at_least: 60 }.node_id();

  let ship = node_named(&state, "Ship v1");
  let quest = base::claiming_quests(state.lock().graph(), ship)[0];
  state.set_active_quest(Some(quest));
  let scene = state.scene();
  let node = scene.nodes.iter().find(|n| n.node == free).unwrap();
  assert_eq!(node.quest, Membership::None);
  assert!(node.dimmed, "pulled in, not claimed");
}

/// Every box is marked with how it belongs to the quests, across all of
/// them and whatever the lens: claimed, required by something claimed, or
/// neither.
#[test]
fn nodes_are_marked_with_their_quest_membership() {
  let mut state = AppState::new(demo_store());
  let ship = node_named(&state, "Ship v1");
  let backend = node_named(&state, "Build backend");
  let membership = |state: &AppState, node| {
    state
      .scene()
      .nodes
      .iter()
      .find(|n| n.node == node)
      .map(|n| n.quest)
  };

  // "v1 Launch" claims Ship v1, which requires everything else.
  assert_eq!(membership(&state, ship), Some(Membership::Direct));
  assert!(
    state
      .scene()
      .nodes
      .iter()
      .filter(|n| n.node != ship)
      .all(|n| n.quest == Membership::Indirect)
  );

  // Claimed by a second quest, Build backend is direct, in its lens too.
  state.select(Some(backend));
  state.new_quest_with_selected();
  assert_eq!(membership(&state, backend), Some(Membership::Direct));

  // Out of every quest, nothing is marked.
  let launch = base::claiming_quests(state.lock().graph(), ship)[0];
  state.set_active_quest(None);
  state.select(Some(ship));
  state.unclaim_selected(launch);
  assert_eq!(membership(&state, ship), Some(Membership::None));
  assert_eq!(
    membership(&state, node_named(&state, "Design schema")),
    Some(Membership::Indirect)
  );
  assert_eq!(
    membership(&state, node_named(&state, "Build frontend")),
    Some(Membership::None)
  );
}
