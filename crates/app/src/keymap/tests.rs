use ui_events::keyboard::{
  Code, Key, KeyState, KeyboardEvent, Modifiers, NamedKey,
};

use super::{Binding, Command, Direction, Flags};
use crate::{camera::ZoomStep, focus::FieldKey};

fn down(key: Key, code: Code, modifiers: Modifiers) -> KeyboardEvent {
  KeyboardEvent {
    modifiers,
    ..KeyboardEvent::key_down(key, code)
  }
}

fn named(k: NamedKey) -> KeyboardEvent {
  down(Key::Named(k), Code::Unidentified, Modifiers::empty())
}

const CMD: Modifiers = if cfg!(target_os = "macos") {
  Modifiers::META
} else {
  Modifiers::CONTROL
};

const SELECTED: Flags = Flags {
  selection: true,
  query:     false,
};
const QUERY: Flags = Flags {
  selection: true,
  query:     true,
};

fn character(s: &str, m: Modifiers) -> KeyboardEvent {
  down(Key::Character(s.into()), Code::Unidentified, m)
}

/// With a search list open, the keys its field passes through drive the
/// list, letters are not shortcuts, and chords still work.
#[test]
fn an_open_search_list_takes_the_list_keys() {
  use Command::{Accept, Move};
  assert_eq!(
    Binding::for_key(&named(NamedKey::ArrowDown), QUERY),
    Some(Binding::Run(Move(1)))
  );
  assert_eq!(
    Binding::for_key(&named(NamedKey::Enter), QUERY),
    Some(Binding::Run(Accept))
  );
  // A stray letter does nothing, though a node is selected.
  assert_eq!(
    Binding::for_key(&character("n", Modifiers::empty()), QUERY),
    None
  );
  assert_eq!(
    Binding::for_key(&named(NamedKey::Escape), QUERY),
    Some(Binding::Run(Command::Escape))
  );
  let undo = down(Key::Character("z".into()), Code::KeyZ, CMD);
  assert_eq!(
    Binding::for_key(&undo, QUERY),
    Some(Binding::Run(Command::Undo))
  );
}

#[test]
fn undo_and_redo_use_the_command_modifier() {
  let z = |m| down(Key::Character("z".into()), Code::KeyZ, m);
  assert_eq!(
    Binding::for_key(&z(CMD), Flags::default()),
    Some(Binding::Run(Command::Undo))
  );
  assert_eq!(
    Binding::for_key(&z(CMD | Modifiers::SHIFT), Flags::default()),
    Some(Binding::Run(Command::Redo))
  );
  let y = down(Key::Character("y".into()), Code::KeyY, CMD);
  assert_eq!(
    Binding::for_key(&y, Flags::default()),
    Some(Binding::Run(Command::Redo))
  );
  // A bare z is not undo.
  assert_eq!(
    Binding::for_key(&z(Modifiers::empty()), Flags::default()),
    None
  );
}

/// Some platforms report Ctrl+Z as a control character; the physical key
/// still identifies it.
#[test]
fn control_characters_fall_back_to_the_physical_key() {
  let ctrl_z = down(Key::Character("\u{1a}".into()), Code::KeyZ, CMD);
  assert_eq!(
    Binding::for_key(&ctrl_z, Flags::default()),
    Some(Binding::Run(Command::Undo))
  );
}

#[test]
fn selection_keys_need_a_selection() {
  for k in [NamedKey::Delete, NamedKey::Backspace] {
    assert_eq!(Binding::for_key(&named(k), Flags::default()), None);
    assert_eq!(
      Binding::for_key(&named(k), SELECTED),
      Some(Binding::Run(Command::Delete))
    );
  }
  for k in [NamedKey::Enter, NamedKey::F2] {
    assert_eq!(Binding::for_key(&named(k), Flags::default()), None);
    assert_eq!(
      Binding::for_key(&named(k), SELECTED),
      Some(Binding::Focus(FieldKey::Title))
    );
  }
}

#[test]
fn r_opens_the_requirement_search_for_a_selection() {
  let r = character("r", Modifiers::empty());
  assert_eq!(Binding::for_key(&r, Flags::default()), None);
  assert_eq!(
    Binding::for_key(&r, SELECTED),
    Some(Binding::Focus(FieldKey::LinkSearch))
  );
  // Not while a search list is open.
  assert_eq!(Binding::for_key(&r, QUERY), None);
}

#[test]
fn ctrl_k_and_slash_open_the_palette() {
  let k = down(Key::Character("k".into()), Code::KeyK, CMD);
  assert_eq!(
    Binding::for_key(&k, Flags::default()),
    Some(Binding::Run(Command::Palette { nodes_only: false }))
  );
  // Also from inside an open query, where it closes the palette.
  assert_eq!(
    Binding::for_key(&k, QUERY),
    Some(Binding::Run(Command::Palette { nodes_only: false }))
  );
  let slash = character("/", Modifiers::empty());
  assert_eq!(
    Binding::for_key(&slash, Flags::default()),
    Some(Binding::Run(Command::Palette { nodes_only: true }))
  );
  // Not while a search list is open.
  assert_eq!(Binding::for_key(&slash, QUERY), None);
}

#[test]
fn single_letters_and_chords_run_their_commands() {
  let plain = |s: &str| character(s, Modifiers::empty());
  let run = |c| Some(Binding::Run(c));
  let none = Flags::default();
  assert_eq!(
    Binding::for_key(&plain("n"), none),
    run(Command::New { condition: false })
  );
  assert_eq!(
    Binding::for_key(&character("N", Modifiers::SHIFT), none),
    run(Command::New { condition: true })
  );
  assert_eq!(Binding::for_key(&plain("f"), none), run(Command::Fit));
  assert_eq!(Binding::for_key(&plain("q"), none), run(Command::Quests));
  assert_eq!(Binding::for_key(&plain("a"), none), run(Command::Now));
  // Space and the arrows act on a selection only.
  assert_eq!(Binding::for_key(&plain(" "), none), None);
  assert_eq!(
    Binding::for_key(&plain(" "), SELECTED),
    run(Command::Primary)
  );
  assert_eq!(Binding::for_key(&named(NamedKey::ArrowLeft), none), None);
  assert_eq!(
    Binding::for_key(&named(NamedKey::ArrowLeft), SELECTED),
    run(Command::Nav(Direction::Left))
  );
  let zoom = |s: &str| down(Key::Character(s.into()), Code::Unidentified, CMD);
  assert_eq!(
    Binding::for_key(&zoom("="), none),
    run(Command::Zoom(ZoomStep::In))
  );
  assert_eq!(
    Binding::for_key(&zoom("+"), none),
    run(Command::Zoom(ZoomStep::In))
  );
  assert_eq!(
    Binding::for_key(&zoom("-"), none),
    run(Command::Zoom(ZoomStep::Out))
  );
  assert_eq!(
    Binding::for_key(&zoom("0"), none),
    run(Command::Zoom(ZoomStep::Reset))
  );
  // With a Ctrl chord, a letter is not its plain command.
  assert_eq!(
    Binding::for_key(&down(Key::Character("n".into()), Code::KeyN, CMD), none),
    None
  );
}

#[test]
fn escape_always_resolves_and_releases_never_do() {
  assert_eq!(
    Binding::for_key(&named(NamedKey::Escape), Flags::default()),
    Some(Binding::Run(Command::Escape))
  );
  let mut up = named(NamedKey::Escape);
  up.state = KeyState::Up;
  assert_eq!(Binding::for_key(&up, Flags::default()), None);
}

#[test]
fn c_opens_the_place_picker() {
  let c = character("c", Modifiers::empty());
  assert_eq!(
    Binding::for_key(&c, Flags::default()),
    Some(Binding::Run(Command::Place))
  );
  assert_eq!(Binding::for_key(&c, QUERY), None);
}
