//! A list row with a trailing control that only shows while the row is
//! hovered: a remove button on an edge row, say. Reading a list should not
//! mean reading a column of destructive buttons.
//!
//! The trailing control's room is always reserved, so the row's text never
//! reflows as the pointer moves over it.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NewWidget, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
  },
  kurbo::{Point, Size},
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx, WidgetView,
  core::{
    MessageContext, MessageResult, Mut, View, ViewId, ViewMarker,
    ViewPathTracker,
  },
};

/// A row: `main` filling the width, `trailing` at the right-hand end and
/// stashed unless the row is hovered.
pub struct HoverRowWidget {
  main: WidgetPod<dyn Widget>,
  trailing: WidgetPod<dyn Widget>,
  /// Whether `trailing` is stashed. Masonry will not lay out a stashed
  /// child, so the row tracks it and skips it.
  stashed: bool,
  /// `trailing`'s size when last laid out, so its room stays reserved while
  /// it is stashed. `None` until it has been measured once.
  trailing_size: Option<Size>,
  /// Space kept between `main` and `trailing`.
  gap: f64,
}

impl HoverRowWidget {
  fn main_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, dyn Widget> {
    this.ctx.get_mut(&mut this.widget.main)
  }

  fn trailing_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, dyn Widget> {
    this.ctx.get_mut(&mut this.widget.trailing)
  }
}

impl Widget for HoverRowWidget {
  type Action = ();

  fn update(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &Update,
  ) {
    // Until it has been measured it stays up, so the first layout can size
    // it; that layout then stashes it.
    if let Update::ChildHoveredChanged(hovered) = event
      && self.trailing_size.is_some()
      && self.stashed == *hovered
    {
      self.stashed = !hovered;
      ctx.set_stashed(&mut self.trailing, self.stashed);
      ctx.request_layout();
    }
  }

  fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
    ctx.register_child(&mut self.main);
    ctx.register_child(&mut self.trailing);
  }

  fn layout(
    &mut self,
    ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    bc: &BoxConstraints,
  ) -> Size {
    let trailing = if self.stashed {
      self.trailing_size.unwrap_or_default()
    } else {
      let size = ctx.run_layout(&mut self.trailing, &bc.loosen());
      if self.trailing_size.is_none() && !ctx.is_hovered() {
        // First measurement, with the pointer elsewhere: hide it now.
        self.stashed = true;
        ctx.set_stashed(&mut self.trailing, true);
      }
      self.trailing_size = Some(size);
      size
    };
    let reserve = trailing.width + self.gap;
    let width = bc.max().width;
    let main_width = (width - reserve).max(0.0);
    let main_bc = BoxConstraints::new(
      Size::new(main_width, bc.min().height),
      Size::new(main_width, bc.max().height),
    );
    let main = ctx.run_layout(&mut self.main, &main_bc);
    let height = main.height.max(trailing.height);
    ctx.place_child(
      &mut self.main,
      Point::new(0.0, (height - main.height) / 2.0),
    );
    if !self.stashed {
      ctx.place_child(
        &mut self.trailing,
        Point::new(width - trailing.width, (height - trailing.height) / 2.0),
      );
    }
    bc.constrain(Size::new(width, height))
  }

  fn paint(
    &mut self,
    _ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    _scene: &mut Scene,
  ) {
  }

  fn accessibility_role(&self) -> Role {
    Role::GenericContainer
  }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    _node: &mut AccessNode,
  ) {
  }

  fn children_ids(&self) -> ChildrenIds {
    ChildrenIds::from_slice(&[self.main.id(), self.trailing.id()])
  }
}

// --- the view -----------------------------------------------------------

const MAIN: ViewId = ViewId::new(0);
const TRAILING: ViewId = ViewId::new(1);

/// A row of `main`, with `trailing` shown at its end on hover and `gap`
/// logical pixels kept between them.
pub fn hover_row<M, T>(gap: f64, main: M, trailing: T) -> HoverRow<M, T> {
  HoverRow {
    gap,
    main,
    trailing,
  }
}

/// The view created by [`hover_row`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct HoverRow<M, T> {
  gap: f64,
  main: M,
  trailing: T,
}

impl<M, T> ViewMarker for HoverRow<M, T> {}
impl<State, Action, M, T> View<State, Action, ViewCtx> for HoverRow<M, T>
where
  State: 'static,
  Action: 'static,
  M: WidgetView<State, Action>,
  T: WidgetView<State, Action>,
{
  type Element = Pod<HoverRowWidget>;
  type ViewState = (M::ViewState, T::ViewState);

  fn build(
    &self,
    ctx: &mut ViewCtx,
    app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let (main, main_state) =
      ctx.with_id(MAIN, |ctx| self.main.build(ctx, app_state));
    let (trailing, trailing_state) =
      ctx.with_id(TRAILING, |ctx| self.trailing.build(ctx, app_state));
    let widget = HoverRowWidget {
      main: NewWidget::erased(main.new_widget).to_pod(),
      trailing: NewWidget::erased(trailing.new_widget).to_pod(),
      stashed: false,
      trailing_size: None,
      gap: self.gap,
    };
    (ctx.create_pod(widget), (main_state, trailing_state))
  }

  fn rebuild(
    &self,
    prev: &Self,
    (main_state, trailing_state): &mut Self::ViewState,
    ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) {
    ctx.with_id(MAIN, |ctx| {
      self.main.rebuild(
        &prev.main,
        main_state,
        ctx,
        HoverRowWidget::main_mut(&mut element).downcast(),
        app_state,
      );
    });
    ctx.with_id(TRAILING, |ctx| {
      self.trailing.rebuild(
        &prev.trailing,
        trailing_state,
        ctx,
        HoverRowWidget::trailing_mut(&mut element).downcast(),
        app_state,
      );
    });
  }

  fn teardown(
    &self,
    (main_state, trailing_state): &mut Self::ViewState,
    ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
  ) {
    ctx.with_id(MAIN, |ctx| {
      self.main.teardown(
        main_state,
        ctx,
        HoverRowWidget::main_mut(&mut element).downcast(),
      );
    });
    ctx.with_id(TRAILING, |ctx| {
      self.trailing.teardown(
        trailing_state,
        ctx,
        HoverRowWidget::trailing_mut(&mut element).downcast(),
      );
    });
  }

  fn message(
    &self,
    (main_state, trailing_state): &mut Self::ViewState,
    message: &mut MessageContext,
    mut element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) -> MessageResult<Action> {
    match message.take_first() {
      Some(MAIN) => self.main.message(
        main_state,
        message,
        HoverRowWidget::main_mut(&mut element).downcast(),
        app_state,
      ),
      Some(TRAILING) => self.trailing.message(
        trailing_state,
        message,
        HoverRowWidget::trailing_mut(&mut element).downcast(),
        app_state,
      ),
      _ => MessageResult::Stale,
    }
  }
}
