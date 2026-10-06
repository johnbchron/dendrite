//! The Xilem view that wraps the app in [`KeymapWidget`].

use masonry::core::{NewWidget, Widget, WidgetMut};
use widgets::wrap::{Wrap, Wrapper, wrap};
use xilem::core::{MessageContext, MessageResult};

use super::{Command, Flags, widget::KeymapWidget};

/// Wrap `child` in the key map. `on_command` runs each resolved command.
pub fn keymap<V, F>(flags: Flags, child: V, on_command: F) -> Keymap<V, F> {
  wrap(KeymapProps { flags, on_command }, child)
}

/// The view created by [`keymap`].
pub type Keymap<V, F> = Wrap<KeymapProps<F>, V>;

/// What a [`Keymap`] is built from.
pub struct KeymapProps<F> {
  flags: Flags,
  on_command: F,
}

impl<State, Action, F> Wrapper<State, Action> for KeymapProps<F>
where
  F: Fn(&mut State, Command) -> Action + Send + Sync + 'static,
{
  type Widget = KeymapWidget;

  const ACTIONS: bool = true;

  fn build(&self, child: NewWidget<dyn Widget>) -> KeymapWidget {
    KeymapWidget::new(child, self.flags)
  }

  fn rebuild(&self, _prev: &Self, widget: &mut WidgetMut<'_, KeymapWidget>) {
    KeymapWidget::set_flags(widget, self.flags);
  }

  fn child_mut<'t>(
    widget: &'t mut WidgetMut<'_, KeymapWidget>,
  ) -> WidgetMut<'t, dyn Widget> {
    KeymapWidget::child_mut(widget)
  }

  fn message(
    &self,
    state: &mut State,
    message: &mut MessageContext,
  ) -> MessageResult<Action> {
    match message.take_message::<Command>() {
      Some(command) => {
        MessageResult::Action((self.on_command)(state, *command))
      }
      None => MessageResult::Stale,
    }
  }
}
