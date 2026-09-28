//! What a key means: the pure half of the key map, kept apart from the
//! widget so it can be tested without one.

use ui_events::keyboard::{Key, KeyState, KeyboardEvent, NamedKey};

use super::{Command, Direction};
use crate::{camera::ZoomStep, focus::FieldKey};

/// What a key does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Binding {
  /// Run a command.
  Run(Command),
  /// Put keyboard focus in a field.
  Focus(FieldKey),
}

/// The app state a key's meaning depends on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flags {
  /// Whether a node is selected.
  pub selection: bool,
  /// Whether a popover with a search list is open (the palette, the quest
  /// switcher): the arrows and Enter then drive its list, and single
  /// letters are not shortcuts.
  pub query:     bool,
}

/// How the command modifier is written on this platform, for hints.
const COMMAND_KEY: &str = if cfg!(target_os = "macos") {
  "Cmd"
} else {
  "Ctrl"
};

/// A shortcut hint for tooltips: `key` with the command modifier, such as
/// "Ctrl+Z".
pub fn chord(key: &str) -> String { format!("{COMMAND_KEY}+{key}") }

impl Binding {
  /// What `key` means given `flags`, if anything. Key releases never mean
  /// anything.
  pub fn for_key(key: &KeyboardEvent, flags: Flags) -> Option<Binding> {
    use Binding::{Focus, Run};
    if key.state != KeyState::Down || key.is_composing {
      return None;
    }
    let m = key.modifiers;
    // The platform's command modifier: Cmd on macOS, Ctrl elsewhere.
    let cmd = if cfg!(target_os = "macos") {
      m.meta()
    } else {
      m.ctrl()
    };
    let plain = !cmd && !m.alt() && !m.meta() && !m.ctrl();

    if flags.query {
      if let Some(binding) = Self::for_query_key(&key.key) {
        return Some(binding);
      }
      // Typing belongs to the search field; a letter that reaches here (the
      // field lost focus) is not a shortcut while the list is up.
      if plain && matches!(key.key, Key::Character(_)) {
        return None;
      }
    }

    let selected = flags.selection;
    let letter_is = |c: &str, l: &str| c.eq_ignore_ascii_case(l);
    match &key.key {
      Key::Named(NamedKey::Escape) => Some(Run(Command::Escape)),
      Key::Named(NamedKey::ArrowUp) if plain && selected => {
        Some(Run(Command::Nav(Direction::Up)))
      }
      Key::Named(NamedKey::ArrowDown) if plain && selected => {
        Some(Run(Command::Nav(Direction::Down)))
      }
      Key::Named(NamedKey::ArrowLeft) if plain && selected => {
        Some(Run(Command::Nav(Direction::Left)))
      }
      Key::Named(NamedKey::ArrowRight) if plain && selected => {
        Some(Run(Command::Nav(Direction::Right)))
      }
      Key::Character(c) if plain && selected && c == " " => {
        Some(Run(Command::Primary))
      }
      Key::Character(c) if plain && letter_is(c, "n") => {
        Some(Run(Command::New {
          condition: m.shift(),
        }))
      }
      Key::Character(c) if plain && letter_is(c, "f") => {
        Some(Run(Command::Fit))
      }
      Key::Character(c) if plain && letter_is(c, "q") => {
        Some(Run(Command::Quests))
      }
      Key::Character(c) if plain && letter_is(c, "a") => {
        Some(Run(Command::Now))
      }
      Key::Character(c) if plain && letter_is(c, "c") => {
        Some(Run(Command::Place))
      }
      Key::Character(c) if cmd && (c == "=" || c == "+") => {
        Some(Run(Command::Zoom(ZoomStep::In)))
      }
      Key::Character(c) if cmd && c == "-" => {
        Some(Run(Command::Zoom(ZoomStep::Out)))
      }
      Key::Character(c) if cmd && c == "0" => {
        Some(Run(Command::Zoom(ZoomStep::Reset)))
      }
      Key::Named(NamedKey::Delete | NamedKey::Backspace)
        if plain && flags.selection =>
      {
        Some(Run(Command::Delete))
      }
      Key::Named(NamedKey::Enter | NamedKey::F2)
        if plain && flags.selection =>
      {
        Some(Focus(FieldKey::Title))
      }
      Key::Character(c)
        if plain && flags.selection && c.eq_ignore_ascii_case("r") =>
      {
        Some(Focus(FieldKey::LinkSearch))
      }
      Key::Character(c) if plain && c == "/" => {
        Some(Run(Command::Palette { nodes_only: true }))
      }
      _ if cmd => match Self::letter(key) {
        Some('k') => Some(Run(Command::Palette { nodes_only: false })),
        Some('z') if m.shift() => Some(Run(Command::Redo)),
        Some('z') => Some(Run(Command::Undo)),
        Some('y') => Some(Run(Command::Redo)),
        _ => None,
      },
      _ => None,
    }
  }

  /// What `key` means to an open search list: the keys its single-line
  /// search field lets through.
  fn for_query_key(key: &Key) -> Option<Binding> {
    use Binding::Run;
    match key {
      Key::Named(NamedKey::ArrowUp) => Some(Run(Command::Move(-1))),
      Key::Named(NamedKey::ArrowDown) => Some(Run(Command::Move(1))),
      Key::Named(NamedKey::Enter) => Some(Run(Command::Accept)),
      _ => None,
    }
  }

  /// The letter a key stands for, lower-cased. With Ctrl held some platforms
  /// report a control character rather than the letter, so fall back to the
  /// physical key.
  fn letter(key: &KeyboardEvent) -> Option<char> {
    if let Key::Character(s) = &key.key
      && let Some(c) = s.chars().next()
      && c.is_ascii_alphabetic()
    {
      return Some(c.to_ascii_lowercase());
    }
    let name = format!("{:?}", key.code);
    let rest = name.strip_prefix("Key")?;
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
      (Some(c), None) if c.is_ascii_alphabetic() => {
        Some(c.to_ascii_lowercase())
      }
      _ => None,
    }
  }
}
