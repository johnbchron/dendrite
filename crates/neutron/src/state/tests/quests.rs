use super::*;

#[test]
fn renaming_a_quest_commits_trims_and_undoes() {
  let store = Store::open_in_memory().unwrap();
  let mut state = AppState::new(store);
  state.new_quest_named(String::new());
  let id = state.active_quest.expect("new quest became the lens");
  assert_eq!(
    state.quest_draft, "New quest",
    "draft seeded from the graph"
  );

  // Blank input is held in the draft but not committed.
  state.rename_active_quest_to("   ".into());
  assert_eq!(state.active_quest_summary().unwrap().0, "New quest");

  // Every keystroke commits the trimmed text; the draft keeps what was
  // typed until Enter tidies it.
  for text in ["  S", "  Sh", "  Ship v1  "] {
    state.rename_active_quest_to(text.into());
  }
  assert_eq!(state.active_quest_summary().unwrap().0, "Ship v1");
  assert_eq!(state.quest_draft, "  Ship v1  ");
  state.finish_rename_quest();
  assert_eq!(state.quest_draft, "Ship v1");

  // One undo reverts the whole rename and pulls the draft back with it.
  state.undo();
  assert_eq!(state.active_quest_summary().unwrap().0, "New quest");
  assert_eq!(state.quest_draft, "New quest");

  // Undoing the creation drops the lens rather than leaving it dangling.
  state.undo();
  assert_eq!(state.active_quest, None);
  assert!(state.quest_draft.is_empty());
  assert!(state.lock().graph().quest(id).is_none());
}

#[test]
fn renaming_a_quest_in_the_global_view_is_a_no_op() {
  let store = Store::open_in_memory().unwrap();
  let mut state = AppState::new(store);
  state.new_quest_named(String::new());
  state.set_active_quest(None);
  state.rename_active_quest_to("Ship v1".into());
  assert!(
    state.lock().graph().quests().all(|q| q.name == "New quest"),
    "no quest was renamed from the global view"
  );
}

/// The switcher is driven from the keyboard: typing filters, the arrows
/// move the highlight, Enter chooses, and a query no quest matches
/// becomes the name of a new one.
#[test]
fn the_quest_switcher_filters_and_chooses_by_keyboard() {
  let mut state = AppState::new(demo_store());
  state.toggle_picker();
  assert!(state.key_flags().query);
  let labels = |s: &AppState| {
    s.quest_rows()
      .into_iter()
      .map(|r| r.label)
      .collect::<Vec<_>>()
  };
  assert_eq!(labels(&state), ["All nodes", "v1 Launch", "New quest"]);
  assert!(state.quest_rows()[0].current, "the global view is current");

  for c in ["l", "a", "u"] {
    let text = format!("{}{c}", state.quest_query().text);
    state.set_quest_text(text);
  }
  assert_eq!(labels(&state), [
    "v1 Launch",
    "New quest \u{201c}lau\u{201d}"
  ]);
  state.run(Command::Accept);
  assert!(!state.picker_open(), "choosing closes the switcher");
  assert_eq!(state.active_quest_summary().unwrap().0, "v1 Launch");

  // A new quest from the query, chosen with the arrow keys.
  state.toggle_picker();
  state.set_quest_text("Garden".into());
  state.run(Command::Move(5)); // clamps to the last row
  state.run(Command::Accept);
  assert_eq!(state.active_quest_summary().unwrap().0, "Garden");
  assert!(!state.picker_open(), "a named quest needs no rename");
}

/// The palette and the inspector change which quests claim the selection:
/// add it to one, take it out, or start a new one with it in one step.
#[test]
fn quest_membership_changes_from_the_palette_and_the_inspector() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  let quests = |state: &AppState| {
    let names: Vec<String> = state
      .selected_info()
      .unwrap()
      .quests
      .into_iter()
      .map(|q| q.1)
      .collect();
    names
  };
  state.select(Some(backend));
  assert!(quests(&state).is_empty());
  let (launch, _) = state.unclaimed_quests()[0].clone();

  // Offered as "Add ... to", and only while something is selected.
  state.open_palette(false);
  state.set_palette_text("add build backend".into());
  let (rows, _) = state.palette_rows();
  let add = rows
    .iter()
    .find(|r| r.label == "Add Build backend to v1 Launch")
    .expect("an add row");
  assert_eq!(add.act, PaletteAct::Claim(launch));
  state.run_palette(add.act);
  assert_eq!(quests(&state), ["v1 Launch"]);
  assert!(state.unclaimed_quests().is_empty());

  // Now offered the other way round.
  state.open_palette(false);
  state.set_palette_text("remove build backend".into());
  let (rows, _) = state.palette_rows();
  assert!(rows.iter().any(|r| r.act == PaletteAct::Unclaim(launch)
    && r.label == "Remove Build backend from v1 Launch"));
  assert!(!rows.iter().any(|r| r.act == PaletteAct::Claim(launch)));
  state.unclaim_selected(launch);
  assert!(quests(&state).is_empty());

  // A new quest with the node in it is one undo step, and opens on its
  // name.
  state.toggle_quests();
  assert!(state.quests_open());
  state.new_quest_with_selected();
  assert!(!state.quests_open());
  assert_eq!(quests(&state), ["New quest"]);
  assert_eq!(
    state.active_quest_summary().unwrap(),
    ("New quest".into(), 1)
  );
  state.undo();
  assert!(quests(&state).is_empty());
  assert_eq!(state.unclaimed_quests().len(), 1, "the quest is gone too");

  // Nothing selected, nothing offered.
  state.select(None);
  state.set_palette_text("add".into());
  let (rows, _) = state.palette_rows();
  assert!(!rows.iter().any(|r| matches!(r.act, PaletteAct::Claim(_))));
}
