//! A draggable divider: a thin vertical grab area with a hairline handle.
//!
//! A thin leaf Masonry widget plus the Xilem [`View`] that hosts it, built the
//! The widget knows nothing about what it divides: it just reports how far
//! the pointer has travelled since the press, and the app decides what that
//! means.
//!
//! Drags are measured against a **window-space** anchor taken at press time.
//! The divider itself slides as the card resizes, so a delta measured in
//! widget-local coordinates would feed back on itself; and reporting the total
//! offset from a fixed anchor (rather than accumulating per-move deltas) means
//! clamping the card at its minimum or maximum width does not desynchronise
//! the pointer from the divider.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, CursorIcon, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerEvent, PropertiesMut, PropertiesRef,
    QueryCtx, RegisterCtx, Update, UpdateCtx, Widget,
  },
  kurbo::{Affine, Point, Rect},
  peniko::{Brush, Color, Fill},
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx,
  core::{MessageContext, MessageResult, Mut, View, ViewMarker},
};

/// Width of the hairline drawn down the middle of the grab area.
const LINE: f64 = 1.0;

/// What the divider reports to the app.
#[derive(Clone, Debug)]
pub enum DividerAction {
  /// A drag started; the app should snapshot its current card width.
  Begin,
  /// The pointer is now this many logical pixels to the right of where it was
  /// pressed (negative means to the left).
  Drag(f64),
}

// --- the widget ---------------------------------------------------------

/// The divider widget. Its only state is the drag anchor.
pub struct DividerWidget {
  /// Window-space x of the press, or `None` when no drag is in progress.
  press_x: Option<f64>,
  /// Total width: the grab area and the layout footprint, deliberately
  /// wider than the hairline it draws.
  width:   f64,
  /// The hairline's colour.
  color:   Color,
}

impl DividerWidget {
  /// A divider `width` wide with no drag in progress.
  pub fn new(width: f64, color: Color) -> Self {
    Self {
      press_x: None,
      width,
      color,
    }
  }

  /// Swap the look; both are read at paint and layout time.
  fn restyle(&mut self, width: f64, color: Color) {
    self.width = width;
    self.color = color;
  }
}

/// Pointer position of an event as a window-space x coordinate.
fn window_x(
  ctx: &EventCtx<'_>,
  physical: masonry::dpi::PhysicalPosition<f64>,
) -> f64 {
  (ctx.window_transform() * ctx.local_position(physical)).x
}

impl Widget for DividerWidget {
  type Action = DividerAction;

  fn on_pointer_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &PointerEvent,
  ) {
    match event {
      PointerEvent::Down(e) => {
        if e.button != Some(PointerButton::Primary) {
          return;
        }
        ctx.capture_pointer();
        self.press_x = Some(window_x(ctx, e.state.position));
        ctx.submit_action::<DividerAction>(DividerAction::Begin);
        ctx.request_render();
      }
      PointerEvent::Move(u) => {
        if let Some(anchor) = self.press_x {
          let dx = window_x(ctx, u.current.position) - anchor;
          ctx.submit_action::<DividerAction>(DividerAction::Drag(dx));
        }
      }
      PointerEvent::Up(_) | PointerEvent::Cancel(_) => {
        self.press_x = None;
        // The hairline drops back from its dragging highlight.
        ctx.request_render();
      }
      _ => {}
    }
  }

  fn update(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &Update,
  ) {
    // Repaint so the hairline can highlight under the pointer.
    if matches!(event, Update::HoveredChanged(_)) {
      ctx.request_render();
    }
  }

  fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

  fn layout(
    &mut self,
    _ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    bc: &BoxConstraints,
  ) -> masonry::kurbo::Size {
    let max = bc.max();
    let h = if max.height.is_finite() {
      max.height
    } else {
      600.0
    };
    bc.constrain((self.width, h))
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let size = ctx.size();
    // The card's own border marks the edge at rest; the handle only shows
    // while it is being pointed at or dragged.
    if self.press_x.is_none() && !ctx.is_hovered() {
      return;
    }
    let color = self.color;
    let x = (size.width - LINE) / 2.0;
    scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(color),
      None,
      &Rect::new(x, 0.0, x + LINE, size.height),
    );
  }

  fn get_cursor(&self, _ctx: &QueryCtx<'_>, _pos: Point) -> CursorIcon {
    CursorIcon::ColResize
  }

  fn accessibility_role(&self) -> Role { Role::Splitter }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    _node: &mut AccessNode,
  ) {
  }

  fn children_ids(&self) -> ChildrenIds { ChildrenIds::new() }
}

// --- the view -----------------------------------------------------------

/// A Xilem [`View`] hosting the [`DividerWidget`].
pub struct Divider<F> {
  /// Total width of the grab area.
  width:     f64,
  /// The hairline's colour.
  color:     Color,
  on_action: F,
}

/// Construct a divider view `width` wide. The handler is called with each
/// [`DividerAction`]; its return value is wrapped in
/// [`MessageResult::Action`] so the driver re-runs `app_logic`.
pub fn divider<State, Action, F>(
  width: f64,
  color: Color,
  on_action: F,
) -> Divider<impl Fn(&mut State, DividerAction) -> MessageResult<Action>>
where
  F: Fn(&mut State, DividerAction) -> Action + 'static,
{
  Divider {
    width,
    color,
    on_action: move |state: &mut State, action| {
      MessageResult::Action(on_action(state, action))
    },
  }
}

impl<F> ViewMarker for Divider<F> {}

impl<F, State, Action> View<State, Action, ViewCtx> for Divider<F>
where
  F: Fn(&mut State, DividerAction) -> MessageResult<Action> + 'static,
  State: 'static,
  Action: 'static,
{
  type Element = Pod<DividerWidget>;
  type ViewState = ();

  fn build(
    &self,
    ctx: &mut ViewCtx,
    _app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let pod = ctx.with_action_widget(|ctx| {
      ctx.create_pod(DividerWidget::new(self.width, self.color))
    });
    (pod, ())
  }

  fn rebuild(
    &self,
    _prev: &Self,
    (): &mut Self::ViewState,
    _ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    _app_state: &mut State,
  ) {
    // The drag state is the widget's own; only the look comes from here.
    element.widget.restyle(self.width, self.color);
    element.ctx.request_render();
  }

  fn teardown(
    &self,
    (): &mut Self::ViewState,
    ctx: &mut ViewCtx,
    element: Mut<'_, Self::Element>,
  ) {
    ctx.teardown_leaf(element);
  }

  fn message(
    &self,
    (): &mut Self::ViewState,
    message: &mut MessageContext,
    _element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) -> MessageResult<Action> {
    match message.take_message::<DividerAction>() {
      Some(action) => (self.on_action)(app_state, *action),
      None => MessageResult::Stale,
    }
  }
}
