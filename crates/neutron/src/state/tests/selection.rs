use super::*;

#[test]
fn selection_shows_both_edge_directions() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));

  let info = state.selected_info().unwrap();
  // "Build backend" requires "Design schema", which is seeded completed.
  assert_eq!(info.requirements.len(), 1);
  assert_eq!(info.requirements[0].name, "Design schema");
  assert_eq!(info.requirements[0].state, NodeState::Completed);
  // ...and "Ship v1" requires it.
  assert_eq!(info.dependents.len(), 1);
  assert_eq!(info.dependents[0].name, "Ship v1");
  assert_eq!(info.dependents[0].state, NodeState::Blocked);
  assert_eq!(info.dependents[0].other, node_named(&state, "Ship v1"));
}

/// The inspector explains each state, and only a Ready task can be
/// completed.
#[test]
fn reasons_and_primary_actions_follow_the_state() {
  let mut state = AppState::new(demo_store());
  let info = |s: &mut AppState, name: &str| {
    let id = node_named(s, name);
    s.select(Some(id));
    s.selected_info().unwrap()
  };

  let ship = info(&mut state, "Ship v1");
  assert_eq!(ship.state, NodeState::Blocked);
  let Reason::WaitingOn(unmet) = &ship.reason else {
    panic!("{:?}", ship.reason)
  };
  let names: Vec<_> = unmet.iter().map(|(_, n)| n.as_str()).collect();
  assert_eq!(names, ["Build backend", "Build frontend"]);
  assert_eq!(ship.primary, Primary::Complete { enabled: false });
  // Pressing it anyway does nothing.
  state.toggle_selected();
  assert_eq!(state.selected_info().unwrap().state, NodeState::Blocked);

  let backend = info(&mut state, "Build backend");
  assert_eq!(backend.reason, Reason::AllMet(1));
  assert_eq!(backend.primary, Primary::Complete { enabled: true });
  state.toggle_selected();
  let backend = state.selected_info().unwrap();
  assert_eq!(backend.reason, Reason::Completed);
  assert_eq!(backend.primary, Primary::Reopen);

  let signoff = info(&mut state, "Design signed off");
  assert_eq!(signoff.reason, Reason::AwaitingSatisfaction);
  assert_eq!(signoff.primary, Primary::Satisfy);

  // Quests claiming the node, and claim status under a lens.
  let ship = info(&mut state, "Ship v1");
  assert_eq!(ship.claimed, None, "no lens, no claim status");
  let names: Vec<_> = ship.quests.iter().map(|(_, n)| n.as_str()).collect();
  assert_eq!(names, ["v1 Launch"]);
  state.set_active_quest(Some(ship.quests[0].0));
  assert_eq!(state.selected_info().unwrap().claimed, Some(true));
}

#[test]
fn a_cycle_names_its_members() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  let ship = node_named(&state, "Ship v1");
  state.add_edge(backend, ship, EdgeKind::Dependency);
  state.select(Some(backend));
  let info = state.selected_info().unwrap();
  assert_eq!(info.state, NodeState::Cyclic);
  assert_eq!(
    info.reason,
    Reason::CycleWith(vec![(ship, "Ship v1".into())])
  );
  assert_eq!(info.primary, Primary::Complete { enabled: false });
}

#[test]
fn arrow_keys_walk_the_graph_as_drawn() {
  let mut state = AppState::new(demo_store());
  let ship = node_named(&state, "Ship v1");
  let backend = node_named(&state, "Build backend");
  let frontend = node_named(&state, "Build frontend");
  let schema = node_named(&state, "Design schema");
  state.select(Some(backend));
  state.run(Command::Nav(Direction::Up));
  assert_eq!(state.selected, Some(ship), "up: what requires it");
  assert_eq!(state.camera().request, CameraRequest::Reveal(ship));
  state.run(Command::Nav(Direction::Up));
  assert_eq!(state.selected, Some(ship), "nothing above a goal");
  state.select(Some(backend));
  state.run(Command::Nav(Direction::Down));
  assert_eq!(state.selected, Some(schema), "down: what it requires");
  // Along the row, and stopping at its ends.
  state.select(Some(backend));
  let row: Vec<NodeId> = {
    let scene = state.scene();
    let r = scene
      .arrangement
      .rows
      .iter()
      .find(|r| r.contains(&Slot::Node(backend)))
      .unwrap()
      .iter()
      .filter_map(|s| match s {
        Slot::Node(n) => Some(*n),
        _ => None,
      })
      .collect();
    r
  };
  assert!(row.contains(&frontend));
  let i = row.iter().position(|n| *n == backend).unwrap();
  state.run(Command::Nav(Direction::Right));
  assert_eq!(state.selected, row.get(i + 1).copied().or(Some(backend)));
}
