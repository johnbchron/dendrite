//! Surfaces: a ground with a border and, optionally, a shadow, wrapped
//! round a padded child.
//!
//! It exists because Masonry's `SizedBox` has no shadow, and floating
//! chrome wants to be separated from what is under it by elevation.
//!
//! A surface also claims the pointer over its whole area, so clicks on its
//! padding do not fall through to whatever is beneath.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, NewWidget,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget,
    WidgetMut, WidgetPod,
  },
  kurbo::{Affine, Line, Point, Rect, RoundedRect, Size, Stroke, Vec2},
  peniko::{Brush, Color, Fill},
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx, WidgetView,
  core::{
    MessageContext, MessageResult, Mut, View, ViewId, ViewMarker,
    ViewPathTracker,
  },
};

/// Everything a surface paints.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
  /// The ground it fills with.
  pub ground: Color,
  /// Its border, or the hairline beneath it when `flat`.
  pub border: Color,
  /// Corner radius; 0 for a square edge.
  pub radius: f64,
  /// Shadow colour, vertical offset and blur (standard deviation).
  pub shadow: Option<(Color, f64, f64)>,
  /// Draw only a hairline along the lower edge instead of a full border:
  /// for a bar spanning the window, whose sides have no edge to show.
  pub flat:   bool,
}

// --- the widget ---------------------------------------------------------

/// Paints a [`Style`] and lays its child out inside `padding`.
pub struct SurfaceWidget {
  child:   WidgetPod<dyn Widget>,
  style:   Style,
  padding: f64,
}

impl SurfaceWidget {
  fn child_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, dyn Widget> {
    this.ctx.get_mut(&mut this.widget.child)
  }
}

impl Widget for SurfaceWidget {
  type Action = ();

  fn on_pointer_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &PointerEvent,
  ) {
    // A press on the surface itself (not a child) is consumed here, so it
    // neither reaches the canvas beneath nor counts as a click on it.
    if matches!(event, PointerEvent::Down(_)) && ctx.target() == ctx.widget_id()
    {
      ctx.set_handled();
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
    let pad = Size::new(2.0 * self.padding, 2.0 * self.padding);
    let inner = bc.shrink(pad);
    let size = ctx.run_layout(&mut self.child, &inner);
    ctx.place_child(&mut self.child, Point::new(self.padding, self.padding));
    bc.constrain(size + pad)
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let rect = ctx.size().to_rect();
    let s = self.style;
    if let Some((color, dy, blur)) = s.shadow {
      scene.draw_blurred_rounded_rect(
        Affine::translate(Vec2::new(0.0, dy)),
        rect,
        color,
        s.radius,
        blur,
      );
    }
    let shape = RoundedRect::from_rect(rect, s.radius);
    scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(s.ground),
      None,
      &shape,
    );
    if s.flat {
      // A bar only needs its lower edge drawn.
      let y = rect.y1 - 0.5;
      scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        &Brush::Solid(s.border),
        None,
        &Line::new((rect.x0, y), (rect.x1, y)),
      );
    } else {
      let ring = RoundedRect::from_rect(inset(rect, 0.5), s.radius);
      scene.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        &Brush::Solid(s.border),
        None,
        &ring,
      );
    }
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

/// `rect` shrunk by `by` on every side.
fn inset(rect: Rect, by: f64) -> Rect { rect.inset(-by) }

// --- the view -----------------------------------------------------------

const CHILD: ViewId = ViewId::new(0);

/// `child` on a surface painted with `style`, padded by `padding` logical
/// pixels on every side.
pub fn surface<State, Action, V>(
  style: Style,
  padding: f64,
  child: V,
) -> Surface<V>
where
  V: WidgetView<State, Action>,
{
  Surface {
    style,
    padding,
    child,
  }
}

/// The view created by [`surface`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Surface<V> {
  style:   Style,
  padding: f64,
  child:   V,
}

impl<V> ViewMarker for Surface<V> {}
impl<State, Action, V> View<State, Action, ViewCtx> for Surface<V>
where
  State: 'static,
  Action: 'static,
  V: WidgetView<State, Action>,
{
  type Element = Pod<SurfaceWidget>;
  type ViewState = V::ViewState;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let (child, child_state) =
      ctx.with_id(CHILD, |ctx| self.child.build(ctx, app_state));
    let widget = SurfaceWidget {
      child:   NewWidget::erased(child.new_widget).to_pod(),
      style:   self.style,
      padding: self.padding,
    };
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
    if self.style != prev.style {
      element.widget.style = self.style;
      element.ctx.request_paint_only();
    }
    if self.padding != prev.padding {
      element.widget.padding = self.padding;
      element.ctx.request_layout();
    }
    ctx.with_id(CHILD, |ctx| {
      self.child.rebuild(
        &prev.child,
        state,
        ctx,
        SurfaceWidget::child_mut(&mut element).downcast(),
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
        SurfaceWidget::child_mut(&mut element).downcast(),
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
        SurfaceWidget::child_mut(&mut element).downcast(),
        app_state,
      ),
      _ => MessageResult::Stale,
    }
  }
}
