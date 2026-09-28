//! "Satisfied by": turning a condition into another kind in place, from
//! the inspector's form or by typing a formula as its name.

use base::{Atom, Moment, PlaceId};

use super::{facts::ManualClock, *};

/// The demo graph on Monday 28 September 2026, with "Design signed off"
/// (required by "Build frontend") selected.
fn signoff_selected() -> (AppState, NodeId) {
  let clock = ManualClock::at("2026-09-28T12:00Z");
  let mut state = AppState::with_clock(demo_store(), clock);
  let signoff = node_named(&state, "Design signed off");
  state.select(Some(signoff));
  (state, signoff)
}

fn after_oct_1_9am() -> Atom {
  Atom::After {
    at: "2026-10-01T09:00".parse::<Moment>().unwrap(),
  }
}

/// The names of what `node` requires.
fn requirement_names(state: &mut AppState, node: NodeId) -> Vec<String> {
  state.select(Some(node));
  state
    .selected_info()
    .unwrap()
    .requirements
    .iter()
    .map(|r| r.name.clone())
    .collect()
}

#[test]
fn a_manual_condition_becomes_a_date_in_place_and_undoes_in_one_step() {
  let (mut state, signoff) = signoff_selected();
  let frontend = node_named(&state, "Build frontend");
  assert_eq!(state.source_kind(), Some(SourceKind::Manual));

  state.choose_source(SourceKind::Date);
  assert_eq!(state.focus_requests().take(), Some(FieldKey::Source));
  assert_eq!(
    state.source_target(),
    Err("Type a date, like oct 1, friday 9am or 2026-10-01.")
  );
  state.set_source_text("oct 1 9am".into());
  assert_eq!(
    state.source_target().map(|t| t.label()),
    Ok("After Thu 1 Oct 09:00".into())
  );
  state.apply_source();

  let date = after_oct_1_9am().node_id();
  assert_eq!(state.selected, Some(date));
  assert_eq!(state.source_draft(), None, "the form closes");
  assert_eq!(state.source_kind(), Some(SourceKind::Date));
  assert!(state.lock().graph().node(signoff).is_none());
  assert_eq!(requirement_names(&mut state, frontend), [
    "After Thu 1 Oct 09:00"
  ]);
  assert_eq!(state.lock().undo_label(), Some("make automatic"));

  state.undo();
  assert!(state.lock().graph().node(date).is_none());
  assert_eq!(requirement_names(&mut state, frontend), [
    "Design signed off"
  ]);
  state.redo();
  assert_eq!(requirement_names(&mut state, frontend), [
    "After Thu 1 Oct 09:00"
  ]);
}

#[test]
fn claims_move_with_the_condition() {
  let (mut state, _) = signoff_selected();
  let quest = state.quest_rows().iter().find_map(|r| match r.choice {
    QuestChoice::Quest(q) => Some(q),
    _ => None,
  });
  let quest = quest.unwrap();
  state.claim_selected(quest);

  state.choose_source(SourceKind::FreeTime);
  state.set_source_text("1h".into());
  state.apply_source();
  let free = Atom::Free { at_least: 60 }.node_id();
  let store = state.lock();
  assert!(store.graph().quest(quest).unwrap().claims.contains(&free));
}

#[test]
fn two_conditions_that_mean_the_same_become_one() {
  let (mut state, signoff) = signoff_selected();
  // A second manual condition, required by Build backend.
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));
  state.run(Command::New { condition: true });
  state.rename_selected_to("Home".into());
  state.live_edit = None;
  let other = state.selected.unwrap();

  // The first names a new place; the second picks it.
  state.select(Some(signoff));
  state.choose_source(SourceKind::Place);
  assert!(state.source_choices().is_empty());
  assert_eq!(
    state.source_target(),
    Err("Pick a place, or name a new one.")
  );
  state.set_source_text("home".into());
  assert_eq!(
    state.source_target().map(|t| t.label()),
    Ok("At Home (new)".into())
  );
  state.apply_source();
  let home: PlaceId = state.place_choices()[0].0;
  let at_home = Atom::At { place: home }.node_id();

  state.select(Some(other));
  state.choose_source(SourceKind::Place);
  let choices = state.source_choices();
  assert_eq!(choices, [(home.to_u128(), "Home".to_string())]);
  state.pick_source(choices[0].0);
  state.apply_source();

  let store = state.lock();
  let graph = store.graph();
  assert_eq!(graph.places().count(), 1);
  assert!(graph.node(other).is_none());
  assert_eq!(graph.dependents_of(at_home).count(), 2);
}

#[test]
fn a_formula_condition_changes_kind_or_goes_back_to_manual() {
  let (mut state, _) = signoff_selected();
  let frontend = node_named(&state, "Build frontend");
  state.choose_source(SourceKind::Date);
  state.set_source_text("oct 1 9am".into());
  state.apply_source();

  // Reopening the form for its own kind fills it in; applying it as it is
  // changes nothing.
  state.choose_source(SourceKind::Date);
  let draft = state.source_draft().unwrap().clone();
  assert_eq!(
    (draft.text.as_str(), draft.before),
    ("2026-10-01 09:00", false)
  );
  let logged = state.lock().event_count().unwrap();
  state.apply_source();
  assert_eq!(state.lock().event_count().unwrap(), logged);

  // Another date is another node; the old one does not linger.
  state.choose_source(SourceKind::Date);
  state.set_source_before(true);
  state.set_source_text("oct 15".into());
  state.apply_source();
  assert!(
    state
      .lock()
      .graph()
      .node(after_oct_1_9am().node_id())
      .is_none()
  );
  assert_eq!(requirement_names(&mut state, frontend), [
    "Before Thu 15 Oct"
  ]);

  // Back to manual, named as it read.
  let date = state.selected_info().unwrap().requirements[0].other;
  state.select(Some(date));
  state.choose_source(SourceKind::Manual);
  assert_eq!(state.source_draft().unwrap().text, "Before Thu 15 Oct");
  state.set_source_text("Deadline passed".into());
  state.apply_source();
  assert_eq!(state.source_kind(), Some(SourceKind::Manual));
  assert_eq!(state.lock().undo_label(), Some("make manual"));
  assert_eq!(requirement_names(&mut state, frontend), ["Deadline passed"]);
}

#[test]
fn becoming_a_formula_drops_the_conditions_own_requirements() {
  let (mut state, signoff) = signoff_selected();
  let schema = node_named(&state, "Design schema");
  state.add_edge(signoff, schema, base::EdgeKind::Dependency);
  state.choose_source(SourceKind::FreeTime);
  assert_eq!(state.source_drops(), 1);
  state.choose_source(SourceKind::Manual);
  assert_eq!(state.source_drops(), 0, "a manual condition keeps them");

  state.choose_source(SourceKind::FreeTime);
  state.set_source_text("30m".into());
  state.apply_source();
  let free = state.selected.unwrap();
  assert_eq!(state.lock().graph().requirements_of(free).count(), 0);
}

#[test]
fn incomplete_forms_apply_nothing() {
  let (mut state, signoff) = signoff_selected();
  let logged = state.lock().event_count().unwrap();

  state.choose_source(SourceKind::Money);
  assert!(state.source_target().is_err());
  state.set_source_text("fifty".into());
  state.apply_source();
  state.choose_source(SourceKind::Manual);
  state.set_source_text("  ".into());
  state.apply_source();

  assert_eq!(state.lock().event_count().unwrap(), logged);
  assert_eq!(state.selected, Some(signoff));
  state.cancel_source();
  assert_eq!(state.source_draft(), None);
}

#[test]
fn a_condition_named_as_a_formula_becomes_one_on_enter() {
  let (mut state, _) = signoff_selected();
  let frontend = node_named(&state, "Build frontend");
  state.select(Some(frontend));
  state.run(Command::New { condition: true });
  state.rename_selected_to("after oct 1 9am".into());
  let offers = state.title_offers();
  assert_eq!(offers[0].label, "After Thu 1 Oct 09:00");
  assert!(state.title_reading().is_some());

  state.finish_rename_selected();
  assert_eq!(state.selected, Some(after_oct_1_9am().node_id()));
  assert_eq!(state.lock().undo_label(), Some("make automatic"));
  assert!(
    requirement_names(&mut state, frontend)
      .contains(&"After Thu 1 Oct 09:00".to_string())
  );
}

#[test]
fn a_loose_match_is_offered_but_not_taken() {
  let (mut state, signoff) = signoff_selected();
  // A context called Online exists.
  state.commit(vec![Event::ContextDefined {
    context: base::ContextId::new(),
    name:    "Online".into(),
  }]);
  state.select(Some(signoff));

  // "on" could mean Online, but only an exact name or "with …" says so.
  state.rename_selected_to("on".into());
  assert!(state.title_offers().iter().any(|o| o.label == "Online"));
  assert_eq!(state.title_reading(), None);
  state.finish_rename_selected();
  assert_eq!(state.selected, Some(signoff));
  assert_eq!(state.source_kind(), Some(SourceKind::Manual));

  // Its exact name is clear.
  state.rename_selected_to("online".into());
  assert!(state.title_reading().is_some());
  state.finish_rename_selected();
  assert_eq!(state.source_kind(), Some(SourceKind::Context));

  // A task's name is never read as a formula.
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));
  state.rename_selected_to("after oct 1".into());
  assert!(state.title_offers().is_empty());
}
