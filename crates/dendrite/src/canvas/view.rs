//! The Xilem view that hosts [`CanvasWidget`].

use std::sync::Arc;

use xilem::{
  Pod, ViewCtx,
  core::{MessageContext, MessageResult, Mut, View, ViewMarker},
};

use super::{
  Camera, CanvasAction, CanvasScene, Insets, LinkMode, widget::CanvasWidget,
};
use crate::theme::Theme;

/// A Xilem [`View`] hosting the [`CanvasWidget`]. Rebuilds push the latest
/// [`CanvasScene`] into the widget; the widget's [`CanvasAction`]s are routed
/// to `on_action`.
pub struct Canvas<F> {
  scene: Arc<CanvasScene>,
  /// The palette the widget paints with.
  theme: &'static Theme,
  /// The latest camera request; acted on when its epoch changes.
  camera: Camera,
  /// How much of the canvas the chrome covers.
  insets: Insets,
  /// Link mode, while it is armed.
  link: Option<LinkMode>,
  on_action: F,
}

/// Construct a canvas view from a scene, the latest camera request, the
/// area the chrome covers, and an action handler.
///
/// The handler's return value is wrapped in [`MessageResult::Action`], which
/// is what tells the Xilem driver to re-run `app_logic` against the mutated
/// app state. (`RequestRebuild` only re-diffs the *existing* view tree, so a
/// selection change made here would never reach the widgets.)
pub fn canvas<State, Action, F>(
  scene: Arc<CanvasScene>,
  theme: &'static Theme,
  camera: Camera,
  insets: Insets,
  link: Option<LinkMode>,
  on_action: F,
) -> Canvas<impl Fn(&mut State, CanvasAction) -> MessageResult<Action>>
where
  F: Fn(&mut State, CanvasAction) -> Action + 'static,
{
  Canvas {
    scene,
    theme,
    camera,
    insets,
    link,
    on_action: move |state: &mut State, action| {
      MessageResult::Action(on_action(state, action))
    },
  }
}

impl<F> ViewMarker for Canvas<F> {}

impl<F, State, Action> View<State, Action, ViewCtx> for Canvas<F>
where
  F: Fn(&mut State, CanvasAction) -> MessageResult<Action> + 'static,
  State: 'static,
  Action: 'static,
{
  type Element = Pod<CanvasWidget>;
  /// The last camera epoch applied, so each request acts once.
  type ViewState = u64;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    _app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let pod = ctx.with_action_widget(|ctx| {
      let mut w = CanvasWidget::new(self.theme);
      w.set_scene(self.scene.clone());
      w.set_insets(self.insets);
      w.set_link(self.link.clone());
      ctx.create_pod(w)
    });
    // The canvas fits itself on first paint, whatever the request says.
    (pod, self.camera.epoch)
  }

  fn rebuild(
    &self,
    prev: &Self,
    last_epoch: &mut Self::ViewState,
    _ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    _app_state: &mut State,
  ) {
    // The app hands back the same `Arc` while nothing the scene depends on
    // changed, so most rebuilds (panel typing, drags) skip the canvas
    // entirely instead of re-measuring every label.
    if !Arc::ptr_eq(&prev.scene, &self.scene) {
      element.widget.set_scene(self.scene.clone());
      // Labels may have changed, and with them node sizes and placement,
      // which then eases in a frame at a time.
      element.ctx.request_layout();
      element.ctx.request_render();
      element.ctx.request_anim_frame();
    }
    if !std::ptr::eq(prev.theme, self.theme) {
      element.widget.set_theme(self.theme);
      element.ctx.request_render();
    }
    if self.insets != prev.insets {
      element.widget.set_insets(self.insets);
    }
    if self.link != prev.link {
      element.widget.set_link(self.link.clone());
      element.ctx.request_render();
    }
    if *last_epoch != self.camera.epoch {
      *last_epoch = self.camera.epoch;
      element.widget.apply(self.camera.request);
      element.ctx.request_render();
      element.ctx.request_anim_frame();
    }
  }

  fn teardown(
    &self,
    _view_state: &mut Self::ViewState,
    ctx: &mut ViewCtx,
    element: Mut<'_, Self::Element>,
  ) {
    ctx.teardown_leaf(element);
  }

  fn message(
    &self,
    _view_state: &mut Self::ViewState,
    message: &mut MessageContext,
    _element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) -> MessageResult<Action> {
    match message.take_message::<CanvasAction>() {
      Some(action) => (self.on_action)(app_state, *action),
      None => MessageResult::Stale,
    }
  }
}
