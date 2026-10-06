//! The widget: a pass-through container at the root of the window.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, NewWidget,
    PaintCtx, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget,
    WidgetMut, WidgetPod,
  },
  kurbo::Size,
  vello::Scene,
};

use super::{Binding, Command, Flags};
use crate::focus;

/// A pass-through container that turns keys into [`Command`]s.
pub struct KeymapWidget {
  child: WidgetPod<dyn Widget>,
  flags: Flags,
}

impl Widget for KeymapWidget {
  type Action = Command;

  fn on_text_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &TextEvent,
  ) {
    let binding = match event {
      TextEvent::Keyboard(key) => Binding::for_key(key, self.flags),
      _ => None,
    };
    match binding {
      Some(Binding::Run(command)) => {
        ctx.submit_action::<Command>(command);
        ctx.set_handled();
      }
      Some(Binding::Focus(field)) => {
        if let Some(id) = focus::lookup(field) {
          ctx.set_focus(id);
          ctx.set_handled();
        }
      }
      None => {}
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
    ChildrenIds::from_slice(&[self.child.id()])
  }
}

impl KeymapWidget {
  /// Wrap `child`, resolving keys against `flags`.
  pub(super) fn new(child: NewWidget<dyn Widget>, flags: Flags) -> Self {
    Self {
      child: child.to_pod(),
      flags,
    }
  }

  /// Update what the app state says keys mean.
  pub(super) fn set_flags(this: &mut WidgetMut<'_, Self>, flags: Flags) {
    this.widget.flags = flags;
  }

  pub(super) fn child_mut<'t>(
    this: &'t mut WidgetMut<'_, Self>,
  ) -> WidgetMut<'t, dyn Widget> {
    this.ctx.get_mut(&mut this.widget.child)
  }
}
