//! Entrance motion: a floating surface eases the last few pixels into place
//! when it appears, so it reads as arriving rather than blinking on.
//!
//! [`AppearWidget`] wraps a child and, from the moment it is added, animates
//! its own transform from an offset back to zero. It is a transform, not a
//! layout change, so nothing is re-measured while it moves.

use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NewWidget, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
  },
  kurbo::{Affine, Point, Size, Vec2},
  vello::Scene,
};

use crate::wrap::{Wrap, Wrapper, wrap};

/// Where the motion starts from, and how long it takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
  /// Offset, in logical pixels, the child starts at.
  pub from:        Vec2,
  /// Duration in milliseconds.
  pub duration_ms: f64,
}

/// Ease-out cubic: fast start, gentle landing.
fn ease_out(t: f64) -> f64 { 1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3) }

/// The offset `motion` has reached after `elapsed_ms`.
fn offset_at(motion: Motion, elapsed_ms: f64) -> Vec2 {
  let t = if motion.duration_ms > 0.0 {
    elapsed_ms / motion.duration_ms
  } else {
    1.0
  };
  motion.from * (1.0 - ease_out(t))
}

// --- the widget ---------------------------------------------------------

/// Slides its child into place when added.
pub struct AppearWidget {
  child:   WidgetPod<dyn Widget>,
  motion:  Motion,
  elapsed: f64,
}

impl Widget for AppearWidget {
  type Action = ();

  fn update(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &Update,
  ) {
    if matches!(event, Update::WidgetAdded) {
      self.elapsed = 0.0;
      ctx.set_transform(Affine::translate(self.motion.from));
      ctx.request_anim_frame();
    }
  }

  fn on_anim_frame(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    interval: u64,
  ) {
    // Cap a first-frame stall so the motion is seen rather than skipped.
    self.elapsed += (interval as f64 / 1e6).min(32.0);
    ctx.set_transform(Affine::translate(offset_at(self.motion, self.elapsed)));
    if self.elapsed < self.motion.duration_ms {
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
    _node: &mut AccessNode,
  ) {
  }

  fn children_ids(&self) -> ChildrenIds {
    ChildrenIds::from_slice(&[self.child.id()])
  }
}

// --- the view -----------------------------------------------------------

/// `child`, easing in from `motion.from` when it first appears.
pub fn appear<V>(motion: Motion, child: V) -> Appear<V> { wrap(motion, child) }

/// The view created by [`appear`].
pub type Appear<V> = Wrap<Motion, V>;

impl<State, Action> Wrapper<State, Action> for Motion {
  type Widget = AppearWidget;

  fn build(&self, child: NewWidget<dyn Widget>) -> AppearWidget {
    AppearWidget {
      child:   child.to_pod(),
      motion:  *self,
      elapsed: 0.0,
    }
  }

  fn rebuild(&self, _prev: &Self, widget: &mut WidgetMut<'_, AppearWidget>) {
    widget.widget.motion = *self;
  }

  fn child_mut<'t>(
    widget: &'t mut WidgetMut<'_, AppearWidget>,
  ) -> WidgetMut<'t, dyn Widget> {
    widget.ctx.get_mut(&mut widget.widget.child)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn motion_starts_at_its_offset_and_lands_at_zero() {
    let m = Motion {
      from:        Vec2::new(16.0, 0.0),
      duration_ms: 160.0,
    };
    assert_eq!(offset_at(m, 0.0), m.from);
    assert_eq!(offset_at(m, 160.0), Vec2::ZERO);
    assert_eq!(offset_at(m, 500.0), Vec2::ZERO, "never overshoots");
    // Ease-out: more than half the distance is covered in the first half.
    assert!(offset_at(m, 80.0).x < 8.0);
    let instant = Motion {
      duration_ms: 0.0,
      ..m
    };
    assert_eq!(offset_at(instant, 0.0), Vec2::ZERO);
  }
}
