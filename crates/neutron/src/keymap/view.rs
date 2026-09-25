//! The Xilem view that wraps the app in [`KeymapWidget`].

use masonry::core::NewWidget;
use xilem::{
  Pod, ViewCtx, WidgetView,
  core::{
    MessageContext, MessageResult, Mut, View, ViewId, ViewMarker,
    ViewPathTracker,
  },
};

use super::{Command, Flags, widget::KeymapWidget};

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
    let widget =
      KeymapWidget::new(NewWidget::erased(child.new_widget), self.flags);
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
    KeymapWidget::set_flags(&mut element, self.flags);
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
