//! Tooltips: a short label that appears under a control after the pointer
//! rests on it.
//!
//! The tooltip is drawn in a Masonry *layer* — a separate widget tree above
//! the whole window — so it is never clipped by the bar or card its control
//! sits in. [`TooltipWidget`] wraps the control, waits [`DELAY_MS`] of
//! hovering, measures the text, and places the layer centred under the
//! control (or right-aligned, for controls at the window's right edge).

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx, LayoutCtx,
    NewWidget, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetId, WidgetMut,
    WidgetPod, render_text,
  },
  kurbo::{Affine, Point, RoundedRect, Size, Stroke},
  parley::{FontContext, Layout as TextLayout, LayoutContext},
  peniko::{Brush, Color, Fill},
  properties::Padding,
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx, WidgetView,
  core::{
    MessageContext, MessageResult, Mut, View, ViewId, ViewMarker,
    ViewPathTracker,
  },
};

use crate::{
  font,
  theme::Theme,
  tokens::{radius, space, text},
};

/// How long the pointer rests on a control before its tooltip shows.
const DELAY_MS: f64 = 450.0;
/// Gap between the control's lower edge and the tooltip.
const GAP: f64 = 6.0;

/// Where the tooltip sits relative to its control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
  /// Centred under the control.
  Center,
  /// Under the control, right edges aligned: for controls near the window's
  /// right edge, where a centred tooltip would run off it.
  End,
}

/// The tooltip's colours.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Colors {
  ground: Color,
  border: Color,
  text:   Color,
}

impl Colors {
  fn from_theme(theme: &Theme) -> Self {
    Self {
      ground: theme.surface_raised,
      border: theme.rule,
      text:   theme.text,
    }
  }
}

// --- the widget ---------------------------------------------------------

/// Wraps a control and shows its tooltip on hover.
pub struct TooltipWidget {
  child:   WidgetPod<dyn Widget>,
  text:    String,
  anchor:  Anchor,
  colors:  Colors,
  /// How long the pointer has rested here, while it is here and the tooltip
  /// is not yet showing.
  resting: Option<f64>,
  /// The layer showing the tooltip, if it is up.
  layer:   Option<WidgetId>,
}

impl TooltipWidget {
  fn child_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, dyn Widget> {
    this.ctx.get_mut(&mut this.widget.child)
  }

  /// Put the tooltip up under the control.
  fn show(&mut self, ctx: &mut UpdateCtx<'_>) {
    if self.text.is_empty() {
      return;
    }
    let pad = Padding::from_vh(space::XS, space::S);
    // Measured here too, to know where to put it: the box's width is the
    // text's plus padding and the 1 px border.
    let width = measure(ctx, &self.text) + pad.left + pad.right + 2.0;
    let origin = ctx.window_origin();
    let size = ctx.size();
    let x = match self.anchor {
      Anchor::Center => origin.x + (size.width - width) / 2.0,
      Anchor::End => origin.x + size.width - width,
    };
    let at = Point::new(x.max(space::XS), origin.y + size.height + GAP);

    let root = NewWidget::new(TooltipBox {
      text: self.text.clone(),
      colors: self.colors,
      pad,
      layout: TextLayout::new(),
    });
    self.layer = Some(root.id());
    ctx.create_layer(root, at);
  }
}

/// The tooltip itself: the layer's root. A leaf that shapes and paints its
/// text, because a layer root is offered the whole window as tight
/// constraints and every stock container would fill it.
struct TooltipBox {
  text:   String,
  colors: Colors,
  pad:    Padding,
  layout: TextLayout<BrushIndex>,
}

impl Widget for TooltipBox {
  type Action = ();

  fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

  fn layout(
    &mut self,
    ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    _bc: &BoxConstraints,
  ) -> Size {
    let (font_cx, layout_cx) = ctx.text_contexts();
    self.layout = shape(font_cx, layout_cx, &self.text);
    // Deliberately ignores the (window-sized) constraints: see above.
    Size::new(
      f64::from(self.layout.width()) + self.pad.left + self.pad.right + 2.0,
      f64::from(self.layout.height()) + self.pad.top + self.pad.bottom + 2.0,
    )
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let rect = ctx.size().to_rect();
    let shape = RoundedRect::from_rect(rect.inset(-0.5), radius::CONTROL);
    scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(self.colors.ground),
      None,
      &shape,
    );
    scene.stroke(
      &Stroke::new(1.0),
      Affine::IDENTITY,
      &Brush::Solid(self.colors.border),
      None,
      &shape,
    );
    render_text(
      scene,
      Affine::translate((self.pad.left + 1.0, self.pad.top + 1.0)),
      &self.layout,
      &[Brush::Solid(self.colors.text)],
      true,
    );
  }

  fn accessibility_role(&self) -> Role { Role::Tooltip }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    node: &mut AccessNode,
  ) {
    node.set_value(self.text.clone());
  }

  fn children_ids(&self) -> ChildrenIds { ChildrenIds::new() }
}

/// Shape `text` as tooltips set it.
fn shape(
  font_cx: &mut FontContext,
  layout_cx: &mut LayoutContext<BrushIndex>,
  text: &str,
) -> TextLayout<BrushIndex> {
  let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
  builder.push_default(StyleProperty::FontSize(text::SECONDARY));
  builder.push_default(StyleProperty::FontStack(font::STACK));
  let mut layout = TextLayout::new();
  builder.build_into(&mut layout, text);
  layout.break_all_lines(None);
  layout
}

/// The width of `text` as the tooltip sets it.
fn measure(ctx: &mut UpdateCtx<'_>, text: &str) -> f64 {
  let (font_cx, layout_cx) = ctx.text_contexts();
  let layout = shape(font_cx, layout_cx, text);
  f64::from(layout.width())
}

impl Widget for TooltipWidget {
  type Action = ();

  fn on_pointer_event(
    &mut self,
    _ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &PointerEvent,
  ) {
    // Pressing the control is the answer to "what does this do?": stop
    // waiting to show it. (A tooltip already up is removed on hover end.)
    if matches!(event, PointerEvent::Down(_)) {
      self.resting = None;
    }
  }

  fn update(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &Update,
  ) {
    if let Update::ChildHoveredChanged(hovered) = event {
      if *hovered {
        self.resting = Some(0.0);
        ctx.request_anim_frame();
      } else {
        self.resting = None;
        if let Some(id) = self.layer.take() {
          ctx.remove_layer(id);
        }
      }
    }
  }

  fn on_anim_frame(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    interval: u64,
  ) {
    let Some(rested) = self.resting.as_mut() else {
      return;
    };
    *rested += interval as f64 / 1e6;
    if *rested >= DELAY_MS {
      self.resting = None;
      if self.layer.is_none() {
        self.show(ctx);
      }
    } else {
      ctx.request_anim_frame();
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
    let size = ctx.run_layout(&mut self.child, bc);
    ctx.place_child(&mut self.child, Point::ORIGIN);
    size
  }

  fn paint(
    &mut self,
    _ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    _scene: &mut Scene,
  ) {
  }

  fn accessibility_role(&self) -> Role { Role::GenericContainer }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    node: &mut AccessNode,
  ) {
    node.set_description(self.text.clone());
  }

  fn children_ids(&self) -> ChildrenIds {
    ChildrenIds::from_slice(&[self.child.id()])
  }
}

// --- the view -----------------------------------------------------------

const CHILD: ViewId = ViewId::new(0);

/// `child` with a tooltip reading `text`.
pub fn tooltip<V>(
  text: impl Into<String>,
  theme: &'static Theme,
  anchor: Anchor,
  child: V,
) -> Tooltip<V> {
  Tooltip {
    text: text.into(),
    theme,
    anchor,
    child,
  }
}

/// The view created by [`tooltip`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Tooltip<V> {
  text:   String,
  theme:  &'static Theme,
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
    let widget = TooltipWidget {
      child:   NewWidget::erased(child.new_widget).to_pod(),
      text:    self.text.clone(),
      anchor:  self.anchor,
      colors:  Colors::from_theme(self.theme),
      resting: None,
      layer:   None,
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
    let changed = self.text != prev.text
      || self.anchor != prev.anchor
      || !std::ptr::eq(self.theme, prev.theme);
    if changed {
      element.widget.text = self.text.clone();
      element.widget.anchor = self.anchor;
      element.widget.colors = Colors::from_theme(self.theme);
      // A tooltip up with the old text comes down; the next hover shows
      // the new one.
      if let Some(id) = element.widget.layer.take() {
        element.ctx.remove_layer(id);
      }
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
    if let Some(id) = element.widget.layer.take() {
      element.ctx.remove_layer(id);
    }
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
