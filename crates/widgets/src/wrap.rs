//! One Xilem view for every widget that wraps a single child.
//!
//! A wrapping view is the same plumbing each time: build the child under a
//! fixed id, build the widget around it, and pass rebuild, teardown and
//! messages through to the child. [`Wrap`] does that once; what differs —
//! the widget, and how it takes new settings — is a [`Wrapper`].

use masonry::core::{FromDynWidget, NewWidget, Widget, WidgetMut};
use xilem::{
  Pod, ViewCtx, WidgetView,
  core::{
    MessageContext, MessageResult, Mut, View, ViewId, ViewMarker,
    ViewPathTracker,
  },
};

const CHILD: ViewId = ViewId::new(0);

/// The settings a wrapping widget is built from, and how to apply them.
pub trait Wrapper<State, Action>: Send + Sync + Sized + 'static {
  /// The widget around the child.
  type Widget: Widget + FromDynWidget;

  /// Whether the widget submits actions of its own, routed to
  /// [`Wrapper::message`].
  const ACTIONS: bool = false;

  /// Make the widget around `child`.
  fn build(&self, child: NewWidget<dyn Widget>) -> Self::Widget;

  /// Bring the widget up to date, from the settings it was built with,
  /// `prev`.
  fn rebuild(&self, prev: &Self, widget: &mut WidgetMut<'_, Self::Widget>);

  /// The widget's child.
  fn child_mut<'t>(
    widget: &'t mut WidgetMut<'_, Self::Widget>,
  ) -> WidgetMut<'t, dyn Widget>;

  /// Called as the widget goes, before its child.
  fn teardown(_widget: &mut WidgetMut<'_, Self::Widget>) {}

  /// An action the widget itself submitted.
  fn message(
    &self,
    _state: &mut State,
    _message: &mut MessageContext,
  ) -> MessageResult<Action> {
    MessageResult::Stale
  }
}

/// `child`, wrapped in the widget `props` builds.
pub fn wrap<P, V>(props: P, child: V) -> Wrap<P, V> {
  Wrap { props, child }
}

/// The view created by [`wrap`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Wrap<P, V> {
  props: P,
  child: V,
}

impl<P, V> ViewMarker for Wrap<P, V> {}
impl<State, Action, P, V> View<State, Action, ViewCtx> for Wrap<P, V>
where
  State: 'static,
  Action: 'static,
  P: Wrapper<State, Action>,
  V: WidgetView<State, Action>,
{
  type Element = Pod<P::Widget>;
  type ViewState = V::ViewState;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let (child, child_state) =
      ctx.with_id(CHILD, |ctx| self.child.build(ctx, app_state));
    let widget = self.props.build(NewWidget::erased(child.new_widget));
    let pod = if P::ACTIONS {
      ctx.with_action_widget(|ctx| ctx.create_pod(widget))
    } else {
      ctx.create_pod(widget)
    };
    (pod, child_state)
  }

  fn rebuild(
    &self,
    prev: &Self,
    state: &mut Self::ViewState,
    ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) {
    self.props.rebuild(&prev.props, &mut element);
    ctx.with_id(CHILD, |ctx| {
      self.child.rebuild(
        &prev.child,
        state,
        ctx,
        P::child_mut(&mut element).downcast(),
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
    P::teardown(&mut element);
    ctx.with_id(CHILD, |ctx| {
      self
        .child
        .teardown(state, ctx, P::child_mut(&mut element).downcast());
    });
    if P::ACTIONS {
      ctx.teardown_leaf(element);
    }
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
        P::child_mut(&mut element).downcast(),
        app_state,
      ),
      None => self.props.message(app_state, message),
      Some(_) => MessageResult::Stale,
    }
  }
}
