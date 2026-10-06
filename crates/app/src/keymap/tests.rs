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

use super::bind::PRIMARY as CMD;

const SELECTED: Flags = Flags {
  selection: true,
  query: false,
};
const QUERY: Flags = Flags {
  selection: true,
  query: true,
};

fn character(s: &str, m: Modifiers) -> KeyboardEvent {
  down(Key::Character(s.into()), Code::Unidentified, m)
}

fn plain(s: &str) -> KeyboardEvent {
  character(s, Modifiers::empty())
}

/// `s` held with the command modifier, on physical key `code`.
fn chord(s: &str, code: Code) -> KeyboardEvent {
  down(Key::Character(s.into()), code, CMD)
}

fn run(c: Command) -> Option<Binding> {
  Some(Binding::Run(c))
}

/// Every key, under every set of flags, and what it should mean.
#[test]
fn keys_resolve_to_their_bindings() {
  use Command::*;
  let none = Flags::default();
  let mut up = named(NamedKey::Escape);
  up.state = KeyState::Up;
  let cases = [
    // With a search list open, the keys its field passes through drive
    // the list, letters are not shortcuts, and chords still work.
    (named(NamedKey::ArrowDown), QUERY, run(Move(1))),
    (named(NamedKey::Enter), QUERY, run(Accept)),
    (plain("n"), QUERY, None),
    (named(NamedKey::Escape), QUERY, run(Escape)),
    (chord("z", Code::KeyZ), QUERY, run(Undo)),
    // Undo and redo use the command modifier; a bare z is not undo.
    (chord("z", Code::KeyZ), none, run(Undo)),
    (
      down(
        Key::Character("z".into()),
        Code::KeyZ,
        CMD | Modifiers::SHIFT,
      ),
      none,
      run(Redo),
    ),
    (chord("y", Code::KeyY), none, run(Redo)),
    (plain("z"), none, None),
    // Some platforms report Ctrl+Z as a control character; the physical
    // key still identifies it.
    (chord("\u{1a}", Code::KeyZ), none, run(Undo)),
    // Selection keys need a selection.
    (named(NamedKey::Delete), none, None),
    (named(NamedKey::Delete), SELECTED, run(Delete)),
    (named(NamedKey::Backspace), none, None),
    (named(NamedKey::Backspace), SELECTED, run(Delete)),
    (named(NamedKey::Enter), none, None),
    (
      named(NamedKey::Enter),
      SELECTED,
      Some(Binding::Focus(FieldKey::Title)),
    ),
    (named(NamedKey::F2), none, None),
    (
      named(NamedKey::F2),
      SELECTED,
      Some(Binding::Focus(FieldKey::Title)),
    ),
    // R opens the requirement search for a selection, but not while a
    // search list is open.
    (plain("r"), none, None),
    (
      plain("r"),
      SELECTED,
      Some(Binding::Focus(FieldKey::LinkSearch)),
    ),
    (plain("r"), QUERY, None),
    // Ctrl+K opens the palette, also from inside an open query, where it
    // closes it; slash does too, but not while a search list is open.
    (
      chord("k", Code::KeyK),
      none,
      run(Palette { nodes_only: false }),
    ),
    (
      chord("k", Code::KeyK),
      QUERY,
      run(Palette { nodes_only: false }),
    ),
    (plain("/"), none, run(Palette { nodes_only: true })),
    (plain("/"), QUERY, None),
    // Single letters and chords run their commands.
    (plain("n"), none, run(New { condition: false })),
    (
      character("N", Modifiers::SHIFT),
      none,
      run(New { condition: true }),
    ),
    (plain("f"), none, run(Fit)),
    (plain("q"), none, run(Quests)),
    (plain("a"), none, run(Now)),
    (plain("c"), none, run(Place)),
    (plain("c"), QUERY, None),
    // Space and the arrows act on a selection only.
    (plain(" "), none, None),
    (plain(" "), SELECTED, run(Primary)),
    (named(NamedKey::ArrowLeft), none, None),
    (
      named(NamedKey::ArrowLeft),
      SELECTED,
      run(Nav(Direction::Left)),
    ),
    (
      chord("=", Code::Unidentified),
      none,
      run(Zoom(ZoomStep::In)),
    ),
    (
      chord("+", Code::Unidentified),
      none,
      run(Zoom(ZoomStep::In)),
    ),
    (
      chord("-", Code::Unidentified),
      none,
      run(Zoom(ZoomStep::Out)),
    ),
    (
      chord("0", Code::Unidentified),
      none,
      run(Zoom(ZoomStep::Reset)),
    ),
    // With a Ctrl chord, a letter is not its plain command.
    (chord("n", Code::KeyN), none, None),
    // Escape always resolves, and releases never do.
    (named(NamedKey::Escape), none, run(Escape)),
    (up, none, None),
  ];
  for (i, (key, flags, want)) in cases.into_iter().enumerate() {
    assert_eq!(Binding::for_key(&key, flags), want, "case {i}: {key:?}");
  }
}
