use super::*;

/// In the global view the tray groups by quest, a node reached by
/// several quests shows under each, and the total counts it once.
#[test]
fn the_now_tray_groups_by_quest_in_the_global_view() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  // A second quest that reaches Build backend too.
  state.new_quest_named("Backend".into());
  let quest = state.active_quest.unwrap();
  state.select(Some(backend));
  state.claim_selected(quest);
  state.set_active_quest(None);

  let (groups, total) = state.now();
  let titles: Vec<_> = groups.iter().map(|g| g.title.clone()).collect();
  assert_eq!(titles, [
    Some("Backend".to_string()),
    Some("v1 Launch".to_string()),
  ]);
  assert!(
    groups
      .iter()
      .all(|g| g.items.iter().any(|(id, _)| *id == backend))
  );
  let distinct: std::collections::HashSet<_> = groups
    .iter()
    .flat_map(|g| g.items.iter().map(|(id, _)| *id))
    .collect();
  assert_eq!(total, distinct.len());

  // A lens shows just its own frontier, untitled.
  state.set_active_quest(state.quest_rows().iter().find_map(
    |r| match r.choice {
      QuestChoice::Quest(q) if r.label == "Backend" => Some(q),
      _ => None,
    },
  ));
  let (groups, total) = state.now();
  assert_eq!(groups.len(), 1);
  assert_eq!(groups[0].title, None);
  assert_eq!(groups[0].items, vec![(
    backend,
    "Build backend".to_string()
  )]);
  assert_eq!(total, 1);
}

#[test]
fn actionable_tracks_the_ready_frontier() {
  let mut state = AppState::new(demo_store());
  // Global view: schema is done, so backend is ready; signoff pending is
  // actionable; frontend/ship blocked.
  let (groups, total) = state.now();
  let names: Vec<String> = groups
    .into_iter()
    .flat_map(|g| g.items)
    .map(|(_, n)| n)
    .collect();
  assert_eq!(names.len(), total);
  assert!(names.contains(&"Build backend".to_string()));
  assert!(!names.contains(&"Ship v1".to_string()));

  // Undo/redo round-trips the store.
  let before = state.scene().nodes.len();
  state.add_task();
  assert_eq!(state.scene().nodes.len(), before + 1);
  state.undo();
  assert_eq!(state.scene().nodes.len(), before);
}
