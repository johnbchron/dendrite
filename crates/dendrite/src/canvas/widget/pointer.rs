//! Pointer input: clicks select, drags pan, the wheel zooms, and hovering
//! previews a link.

use masonry::{
  core::{EventCtx, PointerButton, PointerEvent, ScrollDelta},
  kurbo::Point,
};

use super::CanvasWidget;
use crate::canvas::CanvasAction;

/// Pixels the pointer may travel between press and release and still count
/// as a click rather than a pan.
const CLICK_SLOP: f64 = 4.0;

/// A press in progress. Tracked here rather than read from pointer capture,
/// so panning and clicking work even if capture is not granted.
#[derive(Clone, Copy, Debug)]
pub(super) struct Press {
  /// Where the press went down.
  origin:  Point,
  /// Where the pointer was at the last move, for incremental panning.
  last:    Point,
  /// Whether the gesture has moved far enough to be a pan.
  panning: bool,
}

impl CanvasWidget {
  /// Handle one pointer event.
  pub(super) fn pointer(
    &mut self,
    ctx: &mut EventCtx<'_>,
    event: &PointerEvent,
  ) {
    match event {
      PointerEvent::Down(e) => {
        let p = ctx.local_position(e.state.position);
        ctx.capture_pointer();
        self.press = Some(Press {
          origin:  p,
          last:    p,
          panning: false,
        });
      }
      PointerEvent::Move(u) => {
        let p = ctx.local_position(u.current.position);
        if let Some(press) = &mut self.press {
          if press.origin.distance(p) > CLICK_SLOP {
            press.panning = true;
          }
          if press.panning {
            self.glide = None;
            self.frame.pan += p - press.last;
            ctx.request_render();
          }
          press.last = p;
        } else {
          // Hovering: in link mode, the node under the pointer gets a
          // preview of the edge a click would add.
          let hover = self.hit_test(self.frame.to_world(p));
          if hover != self.hover {
            self.hover = hover;
            if self.link.is_some() {
              ctx.request_render();
            }
          }
        }
      }
      PointerEvent::Leave(_) => {
        if self.hover.take().is_some() && self.link.is_some() {
          ctx.request_render();
        }
      }
      PointerEvent::Up(e) => {
        // A gesture that started on the canvas and never panned is a click:
        // (de)select.
        if let Some(press) = self.press.take()
          && !press.panning
          && e.button == Some(PointerButton::Primary)
        {
          let p = ctx.local_position(e.state.position);
          let hit = self.hit_test(self.frame.to_world(p));
          ctx.submit_action::<CanvasAction>(CanvasAction::Click {
            node:  hit.map(|h| h.node),
            copy:  hit.map(|h| h.copy),
            shift: e.state.modifiers.shift(),
          });
        }
      }
      PointerEvent::Scroll(s) => {
        let p = ctx.local_position(s.state.position);
        let dy = match s.delta {
          ScrollDelta::LineDelta(_, y) => y as f64,
          ScrollDelta::PixelDelta(pos) => pos.y / 40.0,
          _ => 0.0,
        };
        if dy != 0.0 {
          // Scroll up (positive) zooms in. Only the target moves here; the
          // animation frames ease the view after it.
          self.wheel(p, dy);
          ctx.request_anim_frame();
        }
      }
      _ => {}
    }
  }
}
