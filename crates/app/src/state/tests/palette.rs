use super::*;

#[test]
fn palette_commands_and_new_quests_ask_for_the_right_field() {
  let mut state = AppState::new(demo_store());
  let requests = state.focus_requests();
  state.select(Some(node_named(&state, "Build backend")));
  state.run_palette(palette::PaletteAct::Rename);
  assert_eq!(requests.take(), Some(FieldKey::Title));
  state.run_palette(palette::PaletteAct::Require);
  assert!(state.is_linking());
  assert_eq!(requests.take(), Some(FieldKey::LinkSearch));
  // An unnamed quest opens the switcher on its name, not its search.
  state.new_quest_named(String::new());
  assert!(state.picker_open());
  assert_eq!(requests.take(), Some(FieldKey::QuestName));
  // A named one needs neither.
  state.new_quest_named("Named".into());
  assert_eq!(requests.take(), None);
}

#[test]
fn the_palette_finds_nodes_commands_and_quests() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));
  state.select(None);
  state.run(Command::Palette { nodes_only: false });
  assert!(state.palette_open() && state.key_flags().query);

  // Empty: the recent node first, then commands; no quests.
  let (rows, _) = state.palette_rows();
  assert_eq!(rows[0].act, PaletteAct::GoTo(backend));
  assert!(rows.iter().all(|r| r.kind != RowKind::Quest));

  // Typing ranks all three kinds; a quest shows as "Switch to ...".
  state.set_palette_text("laun".into());
  let (rows, _) = state.palette_rows();
  assert!(rows.iter().any(|r| r.label == "Switch to v1 Launch"));
  // Enter on it switches the lens and closes the palette.
  let at = rows
    .iter()
    .position(|r| r.label == "Switch to v1 Launch")
    .unwrap();
  state.run(Command::Move(at as isize));
  state.run(Command::Accept);
  assert!(!state.palette_open());
  assert_eq!(state.active_quest_summary().unwrap().0, "v1 Launch");

  // Going to a node the lens hides leaves the lens.
  let loose = {
    state.set_active_quest(None);
    state.select(None);
    state.add_task();
    state.selected.unwrap()
  };
  let quest = state.quest_rows().iter().find_map(|r| match r.choice {
    QuestChoice::Quest(q) => Some(q),
    _ => None,
  });
  state.set_active_quest(quest);
  state.run_palette(PaletteAct::GoTo(loose));
  assert_eq!(state.active_quest, None);
  assert_eq!(state.selected, Some(loose));
}

#[test]
fn the_slash_palette_lists_nodes_only() {
  let mut state = AppState::new(demo_store());
  state.run(Command::Palette { nodes_only: true });
  state.set_palette_text("e".into());
  let (rows, total) = state.palette_rows();
  assert!(total > 0);
  assert!(rows.iter().all(|r| r.kind == RowKind::Node));
  // Escape closes it.
  state.run(Command::Escape);
  assert!(!state.palette_open());
}
