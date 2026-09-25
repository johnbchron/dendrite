//! The tooltip itself, shown in a layer above the window.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, LayoutCtx, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, UpdateCtx,
    Widget, render_text,
  },
  kurbo::{Affine, RoundedRect, Size, Stroke},
  parley::{FontContext, Layout as TextLayout, LayoutContext},
  peniko::{Brush, Fill},
  properties::Padding,
  vello::Scene,
};

use super::Colors;
use crate::{
  font,
  tokens::{radius, text},
};

/// The tooltip itself: the layer's root. A leaf that shapes and paints its
/// text, because a layer root is offered the whole window as tight
/// constraints and every stock container would fill it.
pub(super) struct TooltipBox {
  text:   String,
  colors: Colors,
  pad:    Padding,
  layout: TextLayout<BrushIndex>,
}

impl TooltipBox {
  /// A box for `text`, shaped at its first layout.
  pub(super) fn new(text: String, colors: Colors, pad: Padding) -> Self {
    Self {
      text,
      colors,
      pad,
      layout: TextLayout::new(),
    }
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
  pub(super) fn measure(ctx: &mut UpdateCtx<'_>, text: &str) -> f64 {
    let (font_cx, layout_cx) = ctx.text_contexts();
    let layout = Self::shape(font_cx, layout_cx, text);
    f64::from(layout.width())
  }
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
    self.layout = Self::shape(font_cx, layout_cx, &self.text);
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
