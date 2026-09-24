//! The key map: global keyboard shortcuts.
//!
//! Masonry sends key events to the focused widget and bubbles them up
//! through its ancestors; when nothing has focus they go to the window's
//! root widget (the "focus fallback"). [`KeymapWidget`] wraps the whole view
//! tree, so it is that root, and it also sees any key a focused text field
//! leaves unhandled. It resolves keys with the pure [`resolve`] and either
//! emits a [`Command`] for the app to run or moves focus to a named field.
//!
//! Single-letter shortcuts only fire while no text field has focus, because
//! a focused field consumes printable keys before they get here.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, NewWidget,
    PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget,
    WidgetMut, WidgetPod,
    keyboard::{Key, KeyState, KeyboardEvent, NamedKey},
  },
  kurbo::Size,
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx, WidgetView,
  core::{
    MessageContext, MessageResult, Mut, View, ViewId, ViewMarker,
    ViewPathTracker,
  },
};

use crate::{
  focus::{self, FieldKey},
  query::QueryEdit,
};

/// Something the app does in response to a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
  /// Undo the last change.
  Undo,
  /// Redo the last undone change.
  Redo,
  /// Delete the selected node.
  Delete,
  /// Back out of whatever is open: link mode, then a popover, then the
  /// selection.
  Escape,
  /// Edit the open query (the quest switcher's search).
  Query(QueryEdit),
  /// Move the open query's highlight up (negative) or down.
  Move(isize),
  /// Act on the open query's highlighted result.
  Accept,
}

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
  /// Whether a popover with a typed query is open: typing, Backspace, the
  /// arrows and Enter then drive the query.
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

/// What `key` means given `flags`, if anything. Key releases never mean
/// anything.
pub fn resolve(key: &KeyboardEvent, flags: Flags) -> Option<Binding> {
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

  if flags.query
    && let Some(binding) = resolve_query(&key.key, plain, cmd || m.alt())
  {
    return Some(binding);
  }

  match &key.key {
    Key::Named(NamedKey::Escape) => Some(Run(Command::Escape)),
    Key::Named(NamedKey::Delete | NamedKey::Backspace)
      if plain && flags.selection =>
    {
      Some(Run(Command::Delete))
    }
    Key::Named(NamedKey::Enter | NamedKey::F2) if plain && flags.selection => {
      Some(Focus(FieldKey::Title))
    }
    _ if cmd => match letter(key) {
      Some('z') if m.shift() => Some(Run(Command::Redo)),
      Some('z') => Some(Run(Command::Undo)),
      Some('y') => Some(Run(Command::Redo)),
      _ => None,
    },
    _ => None,
  }
}

/// What `key` means to an open query, if it is one of the keys a query
/// takes over. `plain` is no modifiers but Shift; `word` is the modifier
/// that makes Backspace delete a word.
fn resolve_query(key: &Key, plain: bool, word: bool) -> Option<Binding> {
  use Binding::Run;
  let edit = |e| Some(Run(Command::Query(e)));
  match key {
    Key::Named(NamedKey::ArrowUp) => Some(Run(Command::Move(-1))),
    Key::Named(NamedKey::ArrowDown) => Some(Run(Command::Move(1))),
    Key::Named(NamedKey::Enter) => Some(Run(Command::Accept)),
    Key::Named(NamedKey::Backspace) if word => edit(QueryEdit::DeleteWord),
    Key::Named(NamedKey::Backspace) => edit(QueryEdit::Backspace),
    Key::Character(s) if plain => edit(QueryEdit::Insert(s.clone())),
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
    (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_lowercase()),
    _ => None,
  }
}

// --- the widget ---------------------------------------------------------

/// A pass-through container that turns keys into [`Command`]s.
pub struct KeymapWidget {
  child: WidgetPod<dyn Widget>,
  flags: Flags,
}

impl Widget for KeymapWidget {
  type Action = Command;

  fn on_text_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &TextEvent,
  ) {
    let binding = match event {
      TextEvent::Keyboard(key) => resolve(key, self.flags),
      // Pasting into an open query types the pasted text.
      TextEvent::ClipboardPaste(text) if self.flags.query => Some(
        Binding::Run(Command::Query(QueryEdit::Insert(text.clone()))),
      ),
      _ => None,
    };
    match binding {
      Some(Binding::Run(command)) => {
        ctx.submit_action::<Command>(command);
        ctx.set_handled();
      }
      Some(Binding::Focus(field)) => {
        if let Some(id) = focus::lookup(field) {
          ctx.set_focus(id);
          ctx.set_handled();
        }
      }
      None => {}
    }
  }

  fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
    ctx.register_child(&mut self.child);
  }

  fn layout(
    &mut self,
    ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    bc: &BoxConstraints,
  ) -> Size {
    let size = ctx.run_layout(&mut self.child, bc);
    ctx.place_child(&mut self.child, (0.0, 0.0).into());
    size
  }

  fn paint(
    &mut self,
    _ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    _scene: &mut Scene,
  ) {
  }

  fn accessibility_role(&self) -> Role { Role::GenericContainer }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    _node: &mut AccessNode,
  ) {
  }

  fn children_ids(&self) -> ChildrenIds {
    ChildrenIds::from_slice(&[self.child.id()])
  }
}

impl KeymapWidget {
  fn child_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, dyn Widget> {
    this.ctx.get_mut(&mut this.widget.child)
  }
}

// --- the view -----------------------------------------------------------

const CHILD: ViewId = ViewId::new(0);

/// Wrap `child` in the key map. `on_command` runs each resolved command.
pub fn keymap<State, Action, V, F>(
  flags: Flags,
  child: V,
  on_command: F,
) -> Keymap<V, F>
where
  V: WidgetView<State, Action>,
  F: Fn(&mut State, Command) -> Action + Send + Sync + 'static,
{
  Keymap {
    flags,
    child,
    on_command,
  }
}

/// The view created by [`keymap`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Keymap<V, F> {
  flags:      Flags,
  child:      V,
  on_command: F,
}

impl<V, F> ViewMarker for Keymap<V, F> {}
impl<State, Action, V, F> View<State, Action, ViewCtx> for Keymap<V, F>
where
  State: 'static,
  Action: 'static,
  V: WidgetView<State, Action>,
  F: Fn(&mut State, Command) -> Action + Send + Sync + 'static,
{
  type Element = Pod<KeymapWidget>;
  type ViewState = V::ViewState;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let (child, child_state) =
      ctx.with_id(CHILD, |ctx| self.child.build(ctx, app_state));
    let widget = KeymapWidget {
      child: NewWidget::erased(child.new_widget).to_pod(),
      flags: self.flags,
    };
    (
      ctx.with_action_widget(|ctx| ctx.create_pod(widget)),
      child_state,
    )
  }

  fn rebuild(
    &self,
    prev: &Self,
    state: &mut Self::ViewState,
    ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) {
    element.widget.flags = self.flags;
    ctx.with_id(CHILD, |ctx| {
      self.child.rebuild(
        &prev.child,
        state,
        ctx,
        KeymapWidget::child_mut(&mut element).downcast(),
        app_state,
      );
    });
  }

  fn teardown(
    &self,
    state: &mut Self::ViewState,
    ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
  ) {
    ctx.with_id(CHILD, |ctx| {
      self.child.teardown(
        state,
        ctx,
        KeymapWidget::child_mut(&mut element).downcast(),
      );
    });
    ctx.teardown_leaf(element);
  }

  fn message(
    &self,
    state: &mut Self::ViewState,
    message: &mut MessageContext,
    mut element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) -> MessageResult<Action> {
    match message.take_first() {
      Some(CHILD) => self.child.message(
        state,
        message,
        KeymapWidget::child_mut(&mut element).downcast(),
        app_state,
      ),
      None => match message.take_message::<Command>() {
        Some(command) => {
          MessageResult::Action((self.on_command)(app_state, *command))
        }
        None => MessageResult::Stale,
      },
      Some(_) => MessageResult::Stale,
    }
  }
}

#[cfg(test)]
mod tests {
  use masonry::core::{Modifiers, keyboard::Code};

  use super::*;

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

  /// With a query open, typing and the list keys drive it, even though a
  /// node is selected and those keys would otherwise delete or rename it.
  #[test]
  fn an_open_query_takes_typing_and_list_keys() {
    use Command::{Accept, Move, Query};
    assert_eq!(
      resolve(&character("S", Modifiers::SHIFT), QUERY),
      Some(Binding::Run(Query(QueryEdit::Insert("S".into()))))
    );
    assert_eq!(
      resolve(&named(NamedKey::Backspace), QUERY),
      Some(Binding::Run(Query(QueryEdit::Backspace)))
    );
    let word = down(Key::Named(NamedKey::Backspace), Code::Backspace, CMD);
    assert_eq!(
      resolve(&word, QUERY),
      Some(Binding::Run(Query(QueryEdit::DeleteWord)))
    );
    assert_eq!(
      resolve(&named(NamedKey::ArrowDown), QUERY),
      Some(Binding::Run(Move(1)))
    );
    assert_eq!(
      resolve(&named(NamedKey::Enter), QUERY),
      Some(Binding::Run(Accept))
    );
    // Escape still backs out, and chords still reach the rest of the map.
    assert_eq!(
      resolve(&named(NamedKey::Escape), QUERY),
      Some(Binding::Run(Command::Escape))
    );
    let undo = down(Key::Character("z".into()), Code::KeyZ, CMD);
    assert_eq!(resolve(&undo, QUERY), Some(Binding::Run(Command::Undo)));
  }

  #[test]
  fn undo_and_redo_use_the_command_modifier() {
    let z = |m| down(Key::Character("z".into()), Code::KeyZ, m);
    assert_eq!(
      resolve(&z(CMD), Flags::default()),
      Some(Binding::Run(Command::Undo))
    );
    assert_eq!(
      resolve(&z(CMD | Modifiers::SHIFT), Flags::default()),
      Some(Binding::Run(Command::Redo))
    );
    let y = down(Key::Character("y".into()), Code::KeyY, CMD);
    assert_eq!(
      resolve(&y, Flags::default()),
      Some(Binding::Run(Command::Redo))
    );
    // A bare z is not undo.
    assert_eq!(resolve(&z(Modifiers::empty()), Flags::default()), None);
  }

  /// Some platforms report Ctrl+Z as a control character; the physical key
  /// still identifies it.
  #[test]
  fn control_characters_fall_back_to_the_physical_key() {
    let ctrl_z = down(Key::Character("\u{1a}".into()), Code::KeyZ, CMD);
    assert_eq!(
      resolve(&ctrl_z, Flags::default()),
      Some(Binding::Run(Command::Undo))
    );
  }

  #[test]
  fn selection_keys_need_a_selection() {
    for k in [NamedKey::Delete, NamedKey::Backspace] {
      assert_eq!(resolve(&named(k), Flags::default()), None);
      assert_eq!(
        resolve(&named(k), SELECTED),
        Some(Binding::Run(Command::Delete))
      );
    }
    for k in [NamedKey::Enter, NamedKey::F2] {
      assert_eq!(resolve(&named(k), Flags::default()), None);
      assert_eq!(
        resolve(&named(k), SELECTED),
        Some(Binding::Focus(FieldKey::Title))
      );
    }
  }

  #[test]
  fn escape_always_resolves_and_releases_never_do() {
    assert_eq!(
      resolve(&named(NamedKey::Escape), Flags::default()),
      Some(Binding::Run(Command::Escape))
    );
    let mut up = named(NamedKey::Escape);
    up.state = KeyState::Up;
    assert_eq!(resolve(&up, Flags::default()), None);
  }
}
