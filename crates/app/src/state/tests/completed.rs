//! The completed lens: finished trees, left out of every other view.

use super::*;

/// The nodes a scene draws, by name.
fn drawn(state: &AppState) -> Vec<String> {
  let mut names: Vec<String> = state
    .scene()
    .nodes
    .iter()
    .map(|n| n.label.clone())
    .collect();
  names.sort();
  names
}

/// Finish every node of the demo graph's one tree, in any order the
/// requirements allow.
fn finish_everything(state: &mut AppState) {
  for name in ["Design signed off", "Build frontend", "Build backend"] {
    state.select(Some(node_named(state, name)));
    state.toggle_selected();
  }
}

/// A tree moves to the completed lens once all its work is done: out of
/// the main view and its quest's lens, and back when any of it is undone.
#[test]
fn a_finished_tree_moves_to_the_completed_lens() {
  let mut state = AppState::new(demo_store());
  let all = drawn(&state);
  state.show_completed();
  assert!(state.scene().nodes.is_empty(), "nothing is finished yet");

  state.set_active_quest(None);
  finish_everything(&mut state);
  assert_eq!(drawn(&state), all, "the root is still to do");

  let ship = node_named(&state, "Ship v1");
  state.select(Some(ship));
  state.toggle_selected();
  assert!(state.scene().nodes.is_empty());
  let launch = base::claiming_quests(state.lock().graph(), ship)[0];
  state.set_active_quest(Some(launch));
  assert!(state.scene().nodes.is_empty(), "nor in its quest");

  state.show_completed();
  assert_eq!(drawn(&state), all);
  assert!(state.scene().nodes.iter().all(|n| !n.dimmed));
  assert_eq!(state.now(), (Vec::new(), 0));
  assert!(
    state
      .quest_rows()
      .iter()
      .any(|r| r.current && r.choice == QuestChoice::Completed)
  );

  state.undo();
  assert!(state.scene().nodes.is_empty(), "Ship v1 is to do again");
  state.set_active_quest(None);
  assert_eq!(drawn(&state), all);
}

/// A node shared with unfinished work stays in the main view, and shows in
/// the completed lens too.
#[test]
fn a_shared_node_stays_with_unfinished_work() {
  let mut state = AppState::new(demo_store());
  // A finished tree of its own that also requires Design schema (done).
  let schema = node_named(&state, "Design schema");
  state.select(None);
  state.run(Command::New { condition: false });
  let docs = state.selected.unwrap();
  state.rename_selected_to("Write docs".into());
  state.live_edit = None;
  state.add_edge(docs, schema, EdgeKind::Dependency);
  state.select(Some(docs));
  state.toggle_selected();

  let main = drawn(&state);
  assert!(!main.contains(&"Write docs".to_string()));
  assert!(main.contains(&"Design schema".to_string()));
  state.show_completed();
  assert_eq!(drawn(&state), ["Design schema", "Write docs"]);
}

/// Going to a node another lens holds switches to that lens.
#[test]
fn revealing_a_node_finds_its_lens() {
  let mut state = AppState::new(demo_store());
  finish_everything(&mut state);
  let ship = node_named(&state, "Ship v1");
  state.select(Some(ship));
  state.toggle_selected();

  state.reveal(ship);
  assert!(state.completed_lens());
  state.select(Some(ship));
  state.toggle_selected();
  state.reveal(ship);
  assert!(!state.completed_lens());
  assert_eq!(state.active_quest, None);
}
