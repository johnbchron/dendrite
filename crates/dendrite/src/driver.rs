//! The app's Masonry driver: xilem's, plus focus requests.
//!
//! Masonry only lets a widget move keyboard focus while it handles an
//! event, so a field that only exists *after* the action that asks for it
//! (the name field of a node just created) cannot be focused from inside
//! the view tree. [`FocusDriver`] wraps xilem's driver: once xilem has run
//! an action and rebuilt the views, it takes any focus request the app
//! filed (see [`FocusRequests`]) and focuses that field through the render
//! root, which can.

use masonry_winit::app::{AppDriver, DriverCtx, MasonryState, WindowId};
use xilem::masonry::core::{ErasedAction, WidgetId};

use crate::focus::{self, FocusRequests};

/// xilem's driver `D`, focusing requested fields after each action.
pub struct FocusDriver<D> {
  inner: D,
  requests: FocusRequests,
}

impl<D> FocusDriver<D> {
  /// Wrap `inner`, serving the requests filed through `requests`.
  pub fn new(inner: D, requests: FocusRequests) -> Self {
    Self { inner, requests }
  }
}

impl<D: AppDriver> AppDriver for FocusDriver<D> {
  fn on_action(
    &mut self,
    window_id: WindowId,
    ctx: &mut DriverCtx<'_, '_>,
    widget_id: WidgetId,
    action: ErasedAction,
  ) {
    self.inner.on_action(window_id, ctx, widget_id, action);
    // By now the views are rebuilt, so a field created by this action has
    // registered itself.
    if let Some(key) = self.requests.take()
      && let Some(id) = focus::lookup(key)
    {
      ctx.render_root(window_id).focus_on(Some(id));
    }
  }

  fn on_start(&mut self, state: &mut MasonryState<'_>) {
    self.inner.on_start(state);
  }

  fn on_close_requested(
    &mut self,
    window_id: WindowId,
    ctx: &mut DriverCtx<'_, '_>,
  ) {
    self.inner.on_close_requested(window_id, ctx);
  }
}
