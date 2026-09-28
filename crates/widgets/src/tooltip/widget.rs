//! The wrapper that watches its control for hover and raises the tooltip.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, NewWidget,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update,
    UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
  },
  kurbo::{Point, Size},
  vello::Scene,
};

use super::{Anchor, DELAY_MS, GAP, Look, bubble::TooltipBox};
/// Wraps a control and shows its tooltip on hover.
pub struct TooltipWidget {
  child:   WidgetPod<dyn Widget>,
  text:    String,
  anchor:  Anchor,
  look:    Look,
  /// How long the pointer has rested here, while it is here and the tooltip
  /// is not yet showing.
  resting: Option<f64>,
  /// The layer showing the tooltip, if it is up.
  layer:   Option<WidgetId>,
}

impl TooltipWidget {
  /// Wrap `child`, whose tooltip reads `text`.
  pub(super) fn new(
    child: NewWidget<dyn Widget>,
    text: String,
    anchor: Anchor,
    look: Look,
  ) -> Self {
    Self {
      child: child.to_pod(),
      text,
      anchor,
      look,
      resting: None,
      layer: None,
    }
  }

  pub(super) fn child_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, dyn Widget> {
    this.ctx.get_mut(&mut this.widget.child)
  }

  /// Change what the tooltip says, where, or in what colours. A tooltip up
  /// with the old text comes down; the next hover shows the new one.
  pub(super) fn relabel(
    this: &mut WidgetMut<'_, Self>,
    text: String,
    anchor: Anchor,
    look: Look,
  ) {
    this.widget.text = text;
    this.widget.anchor = anchor;
    this.widget.look = look;
    Self::hide(this);
  }

  /// Take the tooltip down, if it is up.
  pub(super) fn hide(this: &mut WidgetMut<'_, Self>) {
    if let Some(id) = this.widget.layer.take() {
      this.ctx.remove_layer(id);
    }
  }

  /// Put the tooltip up under the control.
  fn show(&mut self, ctx: &mut UpdateCtx<'_>) {
    if self.text.is_empty() {
      return;
    }
    let pad = self.look.padding;
    // Measured here too, to know where to put it: the box's width is the
    // text's plus padding and the 1 px border.
    let width = TooltipBox::measure(ctx, &self.text, &self.look)
      + pad.left
      + pad.right
      + 2.0;
    let origin = ctx.window_origin();
    let size = ctx.size();
    let x = match self.anchor {
      Anchor::Center => origin.x + (size.width - width) / 2.0,
      Anchor::End => origin.x + size.width - width,
    };
    let at = Point::new(x.max(GAP), origin.y + size.height + GAP);

    let root = NewWidget::new(TooltipBox::new(
      self.text.clone(),
      self.look.clone(),
      pad,
    ));
    self.layer = Some(root.id());
    ctx.create_layer(root, at);
  }
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
