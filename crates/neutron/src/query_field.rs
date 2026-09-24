//! The search box of a popover driven by a typed [`crate::query::Query`].
//!
//! It only *shows* the query: keystrokes reach the query through the key
//! map (see [`crate::keymap`]), not through this widget, so the arrow keys
//! stay free to move the highlight. It is drawn as a focused field — search
//! icon, text or placeholder, caret — because while it is on screen,
//! typing goes to it.

use std::borrow::Cow;

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, LayoutCtx, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, StyleProperty, Widget,
    render_text,
  },
  kurbo::{Affine, Line, RoundedRect, Size, Stroke},
  parley::{
    FontContext, Layout as TextLayout, LayoutContext,
    style::{FontFamily, FontStack},
  },
  peniko::{Brush, Color, Fill},
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx,
  core::{MessageContext, MessageResult, Mut, View, ViewMarker},
};

use crate::{
  font,
  icons::{self, Icon},
  theme::Theme,
  tokens::{radius, size, space, text},
};

/// Height of the box.
const HEIGHT: f64 = 34.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Colors {
  ground: Color,
  ring:   Color,
  text:   Color,
  muted:  Color,
}

impl Colors {
  fn from_theme(theme: &Theme) -> Self {
    Self {
      ground: theme.sunken,
      ring:   theme.focus,
      text:   theme.text,
      muted:  theme.muted,
    }
  }
}

/// A display-only search box.
pub struct QueryFieldWidget {
  text:        String,
  placeholder: String,
  colors:      Colors,
  shaped:      Option<Shaped>,
}

/// The text, placeholder and icon, shaped in the last layout pass.
struct Shaped {
  text:        TextLayout<BrushIndex>,
  placeholder: TextLayout<BrushIndex>,
  icon:        TextLayout<BrushIndex>,
}

fn shape(
  font_cx: &mut FontContext,
  layout_cx: &mut LayoutContext<BrushIndex>,
  text: &str,
  size: f32,
  stack: FontStack<'static>,
) -> TextLayout<BrushIndex> {
  let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
  builder.push_default(StyleProperty::FontSize(size));
  builder.push_default(StyleProperty::FontStack(stack));
  let mut layout = TextLayout::new();
  builder.build_into(&mut layout, text);
  layout.break_all_lines(None);
  layout
}

impl Widget for QueryFieldWidget {
  type Action = ();

  fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

  fn layout(
    &mut self,
    ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    bc: &BoxConstraints,
  ) -> Size {
    let (font_cx, layout_cx) = ctx.text_contexts();
    let icon_stack =
      FontStack::Single(FontFamily::Named(Cow::Borrowed(icons::FAMILY)));
    self.shaped = Some(Shaped {
      text:        shape(
        font_cx,
        layout_cx,
        &self.text,
        text::BODY,
        font::STACK,
      ),
      placeholder: shape(
        font_cx,
        layout_cx,
        &self.placeholder,
        text::BODY,
        font::STACK,
      ),
      icon:        shape(
        font_cx,
        layout_cx,
        &Icon::Search.glyph().to_string(),
        size::ICON,
        icon_stack,
      ),
    });
    let width = if bc.max().width.is_finite() {
      bc.max().width
    } else {
      240.0
    };
    bc.constrain((width, HEIGHT))
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let rect = ctx.size().to_rect();
    let frame = RoundedRect::from_rect(rect.inset(-1.0), radius::CONTROL);
    scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(self.colors.ground),
      None,
      &frame,
    );
    scene.stroke(
      &Stroke::new(2.0),
      Affine::IDENTITY,
      &Brush::Solid(self.colors.ring),
      None,
      &frame,
    );
    let Some(shaped) = &self.shaped else { return };

    let mid = rect.height() / 2.0;
    let icon_h = f64::from(shaped.icon.height());
    render_text(
      scene,
      Affine::translate((space::S, mid - icon_h / 2.0)),
      &shaped.icon,
      &[Brush::Solid(self.colors.muted)],
      true,
    );
    let x = space::S + f64::from(shaped.icon.width()) + space::S;
    let (layout, color) = if self.text.is_empty() {
      (&shaped.placeholder, self.colors.muted)
    } else {
      (&shaped.text, self.colors.text)
    };
    let text_h = f64::from(layout.height());
    render_text(
      scene,
      Affine::translate((x, mid - text_h / 2.0)),
      layout,
      &[Brush::Solid(color)],
      true,
    );
    // The caret sits after the typed text (before the placeholder).
    let caret_x = x
      + if self.text.is_empty() {
        0.0
      } else {
        f64::from(shaped.text.width()) + 1.0
      };
    let half = f64::from(text::BODY) * 0.65;
    scene.stroke(
      &Stroke::new(1.5),
      Affine::IDENTITY,
      &Brush::Solid(self.colors.text),
      None,
      &Line::new((caret_x, mid - half), (caret_x, mid + half)),
    );
  }

  fn accessibility_role(&self) -> Role { Role::SearchInput }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    node: &mut AccessNode,
  ) {
    node.set_value(self.text.clone());
    node.set_placeholder(self.placeholder.clone());
  }

  fn children_ids(&self) -> ChildrenIds { ChildrenIds::new() }
}

// --- the view -----------------------------------------------------------

/// A search box showing `text`, or `placeholder` while it is empty.
pub fn query_field(
  text: impl Into<String>,
  placeholder: impl Into<String>,
  theme: &'static Theme,
) -> QueryField {
  QueryField {
    text: text.into(),
    placeholder: placeholder.into(),
    theme,
  }
}

/// The view created by [`query_field`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct QueryField {
  text:        String,
  placeholder: String,
  theme:       &'static Theme,
}

impl ViewMarker for QueryField {}
impl<State: 'static, Action: 'static> View<State, Action, ViewCtx>
  for QueryField
{
  type Element = Pod<QueryFieldWidget>;
  type ViewState = ();

  fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
    let widget = QueryFieldWidget {
      text:        self.text.clone(),
      placeholder: self.placeholder.clone(),
      colors:      Colors::from_theme(self.theme),
      shaped:      None,
    };
    (ctx.create_pod(widget), ())
  }

  fn rebuild(
    &self,
    prev: &Self,
    _: &mut (),
    _ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    _: &mut State,
  ) {
    if self.text != prev.text || self.placeholder != prev.placeholder {
      element.widget.text = self.text.clone();
      element.widget.placeholder = self.placeholder.clone();
      element.ctx.request_layout();
    }
    if !std::ptr::eq(self.theme, prev.theme) {
      element.widget.colors = Colors::from_theme(self.theme);
      element.ctx.request_paint_only();
    }
  }

  fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

  fn message(
    &self,
    _: &mut (),
    _: &mut MessageContext,
    _: Mut<'_, Self::Element>,
    _: &mut State,
  ) -> MessageResult<Action> {
    MessageResult::Stale
  }
}
