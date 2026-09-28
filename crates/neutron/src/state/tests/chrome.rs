use super::*;

#[test]
fn palette_choice_defaults_persists_and_survives_a_bad_id() {
  let store = db::open_in_memory().unwrap();
  let mut state = AppState::new(store);
  assert_eq!(state.theme().id, Theme::DEFAULT.id, "default when unset");

  state.set_theme("umber");
  assert_eq!(state.theme().id, "umber");
  assert_eq!(
    state.lock().setting("palette").unwrap().as_deref(),
    Some("umber"),
    "the choice was written to the meta table"
  );

  // An id this build does not ship is ignored rather than blanking the UI.
  state.set_theme("chartreuse");
  assert_eq!(state.theme().id, "umber");
}

#[test]
fn a_stored_palette_is_restored_on_open_unless_it_is_unknown() {
  let open_with = |id| {
    let store = db::open_in_memory().unwrap();
    store.set_setting("palette", id).unwrap();
    AppState::new(store).theme().id
  };
  assert_eq!(open_with("meridian"), "meridian");
  assert_eq!(open_with("chartreuse"), Theme::DEFAULT.id);
}

#[test]
fn camera_requests_bump_the_epoch_every_time() {
  let mut state = AppState::new(demo_store());
  let start = state.camera().epoch;
  state.recenter();
  state.recenter();
  assert_eq!(state.camera().epoch, start + 2, "a repeat still acts");
  assert_eq!(state.camera().request, CameraRequest::Fit);

  let backend = node_named(&state, "Build backend");
  state.go_to(backend);
  assert_eq!(state.selected, Some(backend));
  assert_eq!(state.camera().request, CameraRequest::Reveal(backend));
}

#[test]
fn inspector_resize_measures_from_the_press_anchor() {
  let store = db::open_in_memory().unwrap();
  let mut state = AppState::new(store);
  assert_eq!(state.inspector_width(), INSPECTOR_WIDTH);

  // Dragging left widens the panel.
  state.begin_inspector_resize();
  state.resize_inspector(-40.0);
  assert_eq!(state.inspector_width(), INSPECTOR_WIDTH + 40.0);
  // Still the same drag: the delta is total travel, not an increment.
  state.resize_inspector(-60.0);
  assert_eq!(state.inspector_width(), INSPECTOR_WIDTH + 60.0);

  // Overshooting clamps, and coming back does not drift: because the
  // anchor is fixed, returning the pointer restores the original width.
  state.begin_inspector_resize();
  state.resize_inspector(-10_000.0);
  assert_eq!(state.inspector_width(), INSPECTOR_MAX);
  state.resize_inspector(0.0);
  assert_eq!(state.inspector_width(), INSPECTOR_WIDTH + 60.0);

  state.begin_inspector_resize();
  state.resize_inspector(10_000.0);
  assert_eq!(state.inspector_width(), INSPECTOR_MIN);
}

#[test]
fn escape_backs_out_one_layer_at_a_time() {
  let mut state = AppState::new(demo_store());
  let backend = node_named(&state, "Build backend");
  state.select(Some(backend));
  state.begin_link();
  state.toggle_picker();

  state.run(Command::Escape);
  assert!(!state.is_linking(), "link mode goes first");
  assert!(state.picker_open());
  state.run(Command::Escape);
  assert!(!state.picker_open(), "then popovers");
  assert_eq!(state.selected, Some(backend));
  state.run(Command::Escape);
  assert_eq!(state.selected, None, "then the selection");
  assert!(!state.key_flags().selection);
}
