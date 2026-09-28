//! The Xilem view that hosts [`TooltipWidget`].

use masonry::core::NewWidget;
use xilem::{
  Pod, ViewCtx, WidgetView,
  core::{
    MessageContext, MessageResult, Mut, View, ViewId, ViewMarker,
    ViewPathTracker,
  },
};

use super::{Anchor, Look, TooltipWidget};

const CHILD: ViewId = ViewId::new(0);

/// `child` with a tooltip reading `text`.
pub fn tooltip<V>(
  text: impl Into<String>,
  look: Look,
  anchor: Anchor,
  child: V,
) -> Tooltip<V> {
  Tooltip {
    text: text.into(),
    look,
    anchor,
    child,
  }
}

/// The view created by [`tooltip`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Tooltip<V> {
  text:   String,
  look:   Look,
  anchor: Anchor,
  child:  V,
}

impl<V> ViewMarker for Tooltip<V> {}
impl<State, Action, V> View<State, Action, ViewCtx> for Tooltip<V>
where
  State: 'static,
  Action: 'static,
  V: WidgetView<State, Action>,
{
  type Element = Pod<TooltipWidget>;
  type ViewState = V::ViewState;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let (child, child_state) =
      ctx.with_id(CHILD, |ctx| self.child.build(ctx, app_state));
    let widget = TooltipWidget::new(
      NewWidget::erased(child.new_widget),
      self.text.clone(),
      self.anchor,
      self.look.clone(),
    );
    (ctx.create_pod(widget), child_state)
  }

  fn rebuild(
    &self,
    prev: &Self,
    state: &mut Self::ViewState,
    ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) {
    let changed = self.text != prev.text
      || self.anchor != prev.anchor
      || self.look != prev.look;
    if changed {
      TooltipWidget::relabel(
        &mut element,
        self.text.clone(),
        self.anchor,
        self.look.clone(),
      );
    }
    ctx.with_id(CHILD, |ctx| {
      self.child.rebuild(
        &prev.child,
        state,
        ctx,
        TooltipWidget::child_mut(&mut element).downcast(),
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
    // The layer lives outside this widget's tree, so it would outlive it.
    TooltipWidget::hide(&mut element);
    ctx.with_id(CHILD, |ctx| {
      self.child.teardown(
        state,
        ctx,
        TooltipWidget::child_mut(&mut element).downcast(),
      );
    });
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
        TooltipWidget::child_mut(&mut element).downcast(),
        app_state,
      ),
      _ => MessageResult::Stale,
    }
  }
}
