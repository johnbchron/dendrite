//! The custom canvas: a leaf Masonry widget that paints the graph with Vello
//! and a Xilem [`View`](xilem::core::View) that hosts it (PLAN §5,
//! Milestone 3).
//!
//! This is the "custom masonry canvas widget" the PLAN flags as the highest
//! technical risk (§6.1): there is no ready-made canvas in the linebender
//! stack, so we implement [`Widget`](masonry::core::Widget) directly —
//! hit-testing, pan/zoom and Vello painting — and wrap it as a Xilem view
//! that emits [`CanvasAction`]s.
//!
//! The widget is deliberately dumb: it holds a flat [`CanvasScene`] snapshot
//! (positions, shapes, states, edges) that the view recomputes from the
//! global graph on every rebuild. All domain logic lives in `base`; all
//! layout lives in `layout`.
//!
//! - [`scene`]: what the app hands the canvas to draw.
//! - [`camera`]: requests the app makes of the camera, and the transform maths
//!   behind fitting, revealing and easing.
//! - [`labels`]: shaped node labels and the box sizes they give.
//! - [`route`]: where edges attach and the curves they follow.
//! - [`paint`]: drawing nodes and edges into a Vello scene.
//! - [`widget`]: the Masonry widget; [`view`]: the Xilem view.

mod camera;
mod labels;
mod paint;
mod route;
mod scene;
mod view;
mod widget;

pub use self::{
  camera::{Camera, CameraRequest, Insets, ZoomStep},
  scene::{CanvasAction, CanvasScene, LinkMode, RenderEdge, RenderNode},
  view::canvas,
};
