//! A one-shot timer as a view: [`after`] fires its callback once, a delay
//! after it is first built, on the app's async runtime.
//!
//! xilem's `task` view spawns its future once and never again, so it cannot
//! restart for a new toast that replaces an old one. This view is keyed:
//! when the key changes, the pending timer is dropped and a new one starts,
//! and a timer that fires for a key that is no longer current is ignored.
//! It has no widget, so it rides alongside one via `xilem::core::fork`.

use std::{sync::Arc, time::Duration};

use tokio::task::JoinHandle;
use xilem::{
  ViewCtx,
  core::{
    MessageContext, MessageProxy, MessageResult, Mut, NoElement, View, ViewId,
    ViewMarker, ViewPathTracker,
  },
};

/// The message a timer sends itself when it fires, tagged with its key.
#[derive(Debug)]
struct Fired(u64);

/// Call `on_fire` once, `delay` after this view first appears with `key`.
pub fn after<F>(key: u64, delay: Duration, on_fire: F) -> After<F> {
  After {
    key,
    delay,
    on_fire,
  }
}

/// The view created by [`after`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct After<F> {
  key:     u64,
  delay:   Duration,
  on_fire: F,
}

impl<F> After<F> {
  fn spawn(&self, ctx: &mut ViewCtx) -> JoinHandle<()> {
    let path: Arc<[ViewId]> = ctx.view_path().into();
    let proxy = MessageProxy::new(ctx.proxy(), path);
    let (key, delay) = (self.key, self.delay);
    ctx.runtime().spawn(async move {
      tokio::time::sleep(delay).await;
      // The app may be closing; nothing to do if the message is refused.
      let _ = proxy.message(Fired(key));
    })
  }
}

impl<F> ViewMarker for After<F> {}
impl<State, Action, F> View<State, Action, ViewCtx> for After<F>
where
  State: 'static,
  Action: 'static,
  F: Fn(&mut State) -> Action + 'static,
{
  type Element = NoElement;
  type ViewState = JoinHandle<()>;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    _: &mut State,
  ) -> (NoElement, Self::ViewState) {
    (NoElement, self.spawn(ctx))
  }

  fn rebuild(
    &self,
    prev: &Self,
    handle: &mut Self::ViewState,
    ctx: &mut ViewCtx,
    (): Mut<'_, NoElement>,
    _: &mut State,
  ) {
    if self.key != prev.key || self.delay != prev.delay {
      handle.abort();
      *handle = self.spawn(ctx);
    }
  }

  fn teardown(
    &self,
    handle: &mut Self::ViewState,
    _: &mut ViewCtx,
    (): Mut<'_, NoElement>,
  ) {
    handle.abort();
  }

  fn message(
    &self,
    _: &mut Self::ViewState,
    message: &mut MessageContext,
    (): Mut<'_, NoElement>,
    app_state: &mut State,
  ) -> MessageResult<Action> {
    match message.take_message::<Fired>() {
      Some(fired) if fired.0 == self.key => {
        MessageResult::Action((self.on_fire)(app_state))
      }
      // A timer for a toast that has since been replaced.
      Some(_) => MessageResult::Nop,
      None => MessageResult::Stale,
    }
  }
}
