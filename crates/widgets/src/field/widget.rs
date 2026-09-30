//! The widget: a frame painted round a transparent Masonry `TextInput`.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, NewWidget,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
    keyboard::{Key, KeyState, NamedKey},
  },
  kurbo::{RoundedRect, Size},
  util::{fill_color, stroke},
  vello::Scene,
  widgets::{self},
};

use super::{FieldAction, Look};

/// Paints the field's frame around a transparent Masonry `TextInput`, and
/// reports focus changes.
pub struct FieldWidget {
  child: WidgetPod<widgets::TextInput>,
  look: Look,
  /// Whether Escape, after giving up focus, goes on to the key map too.
  escape_bubbles: bool,
  /// Set between a press inside the field and the focus change it causes,
  /// so the view can tell a click from keyboard focus.
  pressed: bool,
}

impl FieldWidget {
  pub(super) fn new(
    child: NewWidget<widgets::TextInput>,
    look: Look,
    escape_bubbles: bool,
  ) -> Self {
    Self {
      child: child.to_pod(),
      look,
      escape_bubbles,
      pressed: false,
    }
  }

  pub(super) fn child_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, widgets::TextInput> {
    this.ctx.get_mut(&mut this.widget.child)
  }

  /// Set whether Escape goes on to the key map after leaving the field.
  pub(super) fn set_escape_bubbles(this: &mut WidgetMut<'_, Self>, on: bool) {
    this.widget.escape_bubbles = on;
  }

  /// Repaint the frame in new colours.
  pub(super) fn set_colors(this: &mut WidgetMut<'_, Self>, look: Look) {
    this.widget.look = look;
    this.ctx.request_paint_only();
  }

  /// Select all of the field's text, so typing replaces it.
  pub(super) fn select_all(this: &mut WidgetMut<'_, Self>) {
    let mut input = Self::child_mut(this);
    let mut area = widgets::TextInput::text_mut(&mut input);
    let len: usize = area.widget.text().into_iter().map(str::len).sum();
    widgets::TextArea::select_byte_range(&mut area, 0, len);
  }
}

impl Widget for FieldWidget {
  type Action = FieldAction;

  fn on_text_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &TextEvent,
  ) {
    // Escape bubbles up from the text area unhandled: treat it as "done
    // here", so the key map sees the next Escape.
    if let TextEvent::Keyboard(key) = event
      && key.state == KeyState::Down
      && key.key == Key::Named(NamedKey::Escape)
      && ctx.has_focus_target()
    {
      ctx.resign_focus();
      if !self.escape_bubbles {
        ctx.set_handled();
      }
    }
    // A submitting Enter bubbles up too (see vendor/README.md), after the
    // text area has reported it: the edit is done, so leave the field. It
    // stops here, since the key map reads Enter as "rename" or "accept".
    if let TextEvent::Keyboard(key) = event
      && key.state == KeyState::Down
      && key.key == Key::Named(NamedKey::Enter)
      && ctx.has_focus_target()
    {
      ctx.resign_focus();
      ctx.set_handled();
    }
  }

  fn on_pointer_event(
    &mut self,
    _ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &PointerEvent,
  ) {
    // Presses bubble up from the text area before focus moves to it.
    match event {
      PointerEvent::Down(_) => self.pressed = true,
      PointerEvent::Up(_) | PointerEvent::Cancel(_) => self.pressed = false,
      _ => {}
    }
  }

  fn update(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &Update,
  ) {
    if let Update::ChildFocusChanged(focused) = event {
      ctx.submit_action::<FieldAction>(FieldAction::Focus {
        focused: *focused,
        by_pointer: std::mem::take(&mut self.pressed),
      });
      ctx.request_paint_only();
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
    ctx.place_child(&mut self.child, (0.0, 0.0).into());
    size
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let rect = ctx.size().to_rect();
    let shape = RoundedRect::from_rect(rect.inset(-0.5), self.look.radius);
    fill_color(scene, &shape, self.look.ground);
    let (color, width) = if ctx.has_focus_target() {
      (self.look.focus, 2.0)
    } else {
      (self.look.border, 1.0)
    };
    // Stroke inside the bounds, so the ring is never clipped by a parent.
    let ring =
      RoundedRect::from_rect(rect.inset(-width / 2.0), self.look.radius);
    stroke(scene, &ring, color, width);
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
    ChildrenIds::from_slice(&[self.child.id()])
  }
}
