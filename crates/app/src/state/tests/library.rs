//! The library: every place, context, resource and schedule, renamed,
//! added and deleted from one popover, and reached from the palette.

use base::Atom;

use super::{facts::ManualClock, *};

fn state() -> AppState {
  AppState::with_clock(demo_store(), ManualClock::at("2026-09-28T12:00Z"))
}

/// The names in the library's section for `kind`.
fn names(state: &AppState, kind: RefKind) -> Vec<String> {
  state
    .library()
    .into_iter()
    .find(|s| s.kind == kind)
    .unwrap()
    .items
    .into_iter()
    .map(|i| i.name)
    .collect()
}

fn item(state: &AppState, name: &str) -> LibraryItem {
  state
    .library()
    .into_iter()
    .flat_map(|s| s.items)
    .find(|i| i.name == name)
    .unwrap_or_else(|| panic!("no referent named {name}"))
}

#[test]
fn a_new_place_opens_on_its_name_and_renames_in_one_step() {
  let mut state = state();
  state.new_referent(RefKind::Place);
  assert!(state.library_open());
  assert_eq!(state.focus_requests().take(), Some(FieldKey::LibraryName));
  assert_eq!(names(&state, RefKind::Place), ["New place"]);

  for text in ["H", "Ho", "Home "] {
    state.set_library_draft(text.into());
  }
  assert_eq!(names(&state, RefKind::Place), ["Home"]);
  assert_eq!(
    state.library_draft(),
    "Home ",
    "the field keeps what's typed"
  );
  state.finish_library_rename();
  assert_eq!(state.library_editing(), None);

  // The rename folds into one step; undoing it restores the default name.
  state.undo();
  assert_eq!(names(&state, RefKind::Place), ["New place"]);

  // A second new place does not reuse the name.
  state.new_referent(RefKind::Place);
  assert_eq!(names(&state, RefKind::Place), ["New place", "New place 2"]);
}

#[test]
fn a_referent_in_use_is_not_deleted_and_lists_its_uses() {
  let mut state = state();
  state.new_referent(RefKind::Context);
  state.set_library_draft("Online".into());
  state.finish_library_rename();
  let online = item(&state, "Online");
  let RefKey::Context(context) = online.key else {
    panic!("not a context")
  };

  // "Design signed off" becomes In(Online).
  let signoff = node_named(&state, "Design signed off");
  state.select(Some(signoff));
  state.choose_source(SourceKind::Context);
  state.pick_source(context.to_u128());
  state.apply_source();
  let atom = Atom::In { context }.node_id();
  assert_eq!(item(&state, "Online").uses, [atom]);

  state.delete_referent(online.key);
  assert_eq!(names(&state, RefKind::Context), ["Online"], "still in use");

  // Following a use selects that condition and closes the library.
  state.toggle_library();
  state.select(None);
  state.show_use(atom);
  assert_eq!(state.selected, Some(atom));
  assert!(!state.library_open());
}

#[test]
fn deleting_the_place_i_am_at_clears_it_and_undoes() {
  let mut state = state();
  state.new_referent(RefKind::Place);
  let key = item(&state, "New place").key;
  let RefKey::Place(place) = key else { panic!() };
  state.set_place(Some(place));
  assert_eq!(item(&state, "New place").note.as_deref(), Some("Here"));

  state.delete_referent(key);
  assert!(names(&state, RefKind::Place).is_empty());
  assert_eq!(state.place(), None);
  assert_eq!(
    state.toast().map(|t| t.text.as_str()),
    Some("Deleted place New place")
  );
  assert_eq!(state.library_editing(), None, "its name field closes");
  state.undo();
  assert_eq!(names(&state, RefKind::Place), ["New place"]);
}

#[test]
fn the_palette_finds_places_and_contexts_by_name() {
  let mut state = state();
  state.new_referent(RefKind::Place);
  state.set_library_draft("Hardware store".into());
  state.new_referent(RefKind::Context);
  state.set_library_draft("Online".into());
  state.close_popovers();

  let rows = |state: &mut AppState, text: &str| {
    state.set_palette_text(text.into());
    state.palette_rows().0
  };
  let found = rows(&mut state, "hardware");
  let at = found
    .iter()
    .find(|r| r.label == "I'm at Hardware store")
    .expect("a row to declare the place");
  assert_eq!(at.kind, RowKind::Referent(RefKind::Place));
  let act = at.act;
  state.run_palette(act);
  assert_eq!(state.place_name().as_deref(), Some("Hardware store"));
  // Once there, it is not offered again.
  assert!(
    !rows(&mut state, "hardware")
      .iter()
      .any(|r| r.label.starts_with("I'm at"))
  );

  let act = rows(&mut state, "turn on online")[0].act;
  state.run_palette(act);
  assert_eq!(state.active_contexts().len(), 1);

  let rename = rows(&mut state, "rename context online")
    .into_iter()
    .find(|r| r.label == "Rename context Online")
    .unwrap();
  state.run_palette(rename.act);
  assert!(state.library_open());
  assert_eq!(state.library_draft(), "Online");
}
