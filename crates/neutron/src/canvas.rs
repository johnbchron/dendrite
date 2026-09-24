//! The custom canvas: a leaf Masonry widget that paints the graph with Vello
//! and a Xilem [`View`] that hosts it (PLAN §5, Milestone 3).
//!
//! This is the "custom masonry canvas widget" the PLAN flags as the highest
//! technical risk (§6.1): there is no ready-made canvas in the linebender
//! stack, so we implement [`Widget`] directly — hit-testing, pan/zoom and
//! Vello painting — and wrap it as a Xilem view that emits [`CanvasAction`]s.
//!
//! The widget is deliberately dumb: it holds a flat [`CanvasScene`] snapshot
//! (positions, shapes, states, edges) that the view recomputes from the
//! global graph on every rebuild. All domain logic lives in `base`; all
//! layout lives in `layout`.

use std::collections::HashMap;

use base::{EdgeKind, NodeId, NodeKind, NodeState};
use layout::{Arrangement, LayoutConfig};
use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, UpdateCtx, Widget, render_text,
  },
  kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Size, Vec2},
  parley::{Layout as TextLayout, LineHeight},
  peniko::{Brush, Color, Fill},
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx,
  core::{MessageContext, MessageResult, Mut, View, ViewMarker},
};

use crate::{
  font,
  theme::{self, Theme},
};

/// Width of every node box in world (graph) units, and so the width its
/// label wraps at. Height follows the label (see [`node_size`]).
const NODE_W: f64 = 150.0;
/// Space between a node's border and its label.
const PAD_X: f64 = 10.0;
const PAD_Y: f64 = 8.0;
/// Label type size, and line height as a multiple of it.
const LABEL_SIZE: f32 = 13.0;
const LABEL_LINE: f32 = 1.3;
/// Lines of label every box leaves room for, however short its text: a
/// one-word node is not a sliver, and a two-line label does not grow its box.
const MIN_LINES: f64 = 2.0;
/// Pixels the pointer may travel between press and release and still count as
/// a click rather than a pan.
const CLICK_SLOP: f64 = 4.0;
/// How far the view zooms in and out.
const ZOOM_MIN: f64 = 0.15;
const ZOOM_MAX: f64 = 4.0;
/// How quickly an animated zoom closes on its target, per second: the gap
/// shrinks by a factor of `e` every `1 / ZOOM_RATE` seconds, so a wheel notch
/// settles in roughly a fifth of a second and rapid notches blend together
/// rather than stepping.
const ZOOM_RATE: f64 = 16.0;

// --- render data --------------------------------------------------------

/// One node as the canvas needs to draw and hit-test it.
#[derive(Clone, Debug)]
pub struct RenderNode {
  /// Which node this is (returned in [`CanvasAction::Select`]).
  pub id:       NodeId,
  /// Display text.
  pub label:    String,
  /// Task vs. condition — selects the shape.
  pub kind:     NodeKind,
  /// Derived status — selects the fill/border colours.
  pub state:    NodeState,
  /// Whether this node is the current selection.
  pub selected: bool,
  /// Whether this node is only pulled into the active quest's scope (not
  /// claimed) — rendered dimmed (PLAN §5).
  pub dimmed:   bool,
}

/// One edge between two rendered nodes, plus styling flags.
#[derive(Clone, Debug)]
pub struct RenderEdge {
  /// The dependent / parent end.
  pub from:     NodeId,
  /// The requirement / child end (the arrow points here).
  pub to:       NodeId,
  /// Dependency (solid) vs. subtask (dashed).
  pub kind:     EdgeKind,
  /// Whether the cycle-cut reversed this edge (a backward cycle edge).
  pub reversed: bool,
}

/// A complete, self-contained description of what to paint.
#[derive(Clone, Debug, Default)]
pub struct CanvasScene {
  /// Nodes, painted on top of edges.
  pub nodes:       Vec<RenderNode>,
  /// Edges, painted underneath.
  pub edges:       Vec<RenderEdge>,
  /// The rows and order the nodes are placed in. Coordinates are assigned
  /// by the widget, which is the only place label sizes are known.
  pub arrangement: Arrangement,
}

/// Something the user did on the canvas that the app must react to.
#[derive(Clone, Debug)]
pub enum CanvasAction {
  /// Selection changed (a node, or `None` when the user clicked empty space).
  Select(Option<NodeId>),
}

// --- the widget ---------------------------------------------------------

/// The Masonry canvas widget. Owns the view transform (pan/zoom) and the
/// interaction state; the scene is pushed in from the view.
pub struct CanvasWidget {
  scene:        CanvasScene,
  /// World→screen translation.
  pan:          Vec2,
  /// World→screen scale.
  zoom:         f64,
  /// The scale the view is animating towards; wheel input moves this, and
  /// each animation frame eases [`Self::zoom`] after it.
  zoom_target:  f64,
  /// Screen point the animated zoom is anchored on (the last wheel position).
  zoom_anchor:  Point,
  /// Pointer position at the last `Down`, for click-vs-pan discrimination.
  press_origin: Option<Point>,
  /// Previous pointer position while dragging, for incremental panning.
  last_pointer: Option<Point>,
  /// Whether the current gesture has moved far enough to be a pan.
  panned:       bool,
  /// When set, the next paint fits the whole graph into the viewport. Set on
  /// construction (so the app opens centred) and whenever a recenter is
  /// requested (PLAN §5, §6.3).
  needs_fit:    bool,
  /// Viewport size from the previous layout pass, so a resize can request
  /// a refit (the existing fit is centred on the old viewport).
  last_size:    Option<Size>,
  /// The palette every colour painted here comes from, pushed in from the
  /// view on rebuild.
  theme:        &'static Theme,
  /// Cached per-node text layouts, keyed by id, invalidated when the label
  /// text changes.
  text_cache:   HashMap<NodeId, (String, TextLayout<BrushIndex>)>,
  /// Every node's box in world coordinates, placed in the layout pass from
  /// the scene's arrangement and the measured labels.
  rects:        HashMap<NodeId, Rect>,
  /// Where each of the scene's edges attaches, index-aligned with
  /// `scene.edges`; routed alongside `rects`.
  routes:       Vec<Option<Route>>,
  /// Whether [`font::install`] has pointed the default family at the app
  /// face yet.
  font_ready:   bool,
}

impl CanvasWidget {
  /// A fresh canvas with an identity transform and no scene.
  pub fn new(theme: &'static Theme) -> Self {
    Self {
      scene: CanvasScene::default(),
      pan: Vec2::new(60.0, 60.0),
      zoom: 1.0,
      zoom_target: 1.0,
      zoom_anchor: Point::ORIGIN,
      press_origin: None,
      last_pointer: None,
      panned: false,
      needs_fit: true,
      last_size: None,
      theme,
      text_cache: HashMap::new(),
      rects: HashMap::new(),
      routes: Vec::new(),
      font_ready: false,
    }
  }

  /// Replace the scene. The caller must request a layout, which re-measures
  /// and re-places the nodes.
  fn set_scene(&mut self, scene: CanvasScene) {
    // Drop cached text for nodes that vanished.
    let live: std::collections::HashSet<NodeId> =
      scene.nodes.iter().map(|n| n.id).collect();
    self.text_cache.retain(|id, _| live.contains(id));
    self.scene = scene;
  }

  /// Swap the palette. Colours are read at paint time, so storing it is the
  /// whole job.
  fn set_theme(&mut self, theme: &'static Theme) { self.theme = theme; }

  /// The current world→screen transform.
  fn transform(&self) -> Affine {
    Affine::translate(self.pan) * Affine::scale(self.zoom)
  }

  /// Map a screen point (widget-local) to world coordinates.
  fn to_world(&self, screen: Point) -> Point {
    self.transform().inverse() * screen
  }

  /// Topmost node whose box contains `world`, searched front-to-back.
  fn hit_test(&self, world: Point) -> Option<NodeId> {
    self
      .scene
      .nodes
      .iter()
      .rev()
      .find(|n| self.rects.get(&n.id).is_some_and(|r| r.contains(world)))
      .map(|n| n.id)
  }

  /// Shape any label not already cached, then size and place every node.
  fn measure(&mut self, ctx: &mut LayoutCtx<'_>) {
    // Masonry's *shared* text contexts, not a private pair: a private
    // `FontContext` resolves against its own font set, so canvas labels
    // would not match the panel and would miss any font the app registers.
    let (font_cx, layout_cx) = ctx.text_contexts();
    for node in &self.scene.nodes {
      let fresh = self
        .text_cache
        .get(&node.id)
        .is_some_and(|(text, _)| text == &node.label);
      if fresh {
        continue;
      }
      let mut builder =
        layout_cx.ranged_builder(font_cx, &node.label, 1.0, true);
      builder.push_default(StyleProperty::FontSize(LABEL_SIZE));
      builder.push_default(StyleProperty::LineHeight(
        LineHeight::FontSizeRelative(LABEL_LINE),
      ));
      // A hand-rolled widget has to ask for the app face itself, or parley
      // picks its own default and the canvas ends up in a different typeface
      // to the panel.
      builder.push_default(StyleProperty::FontStack(font::STACK));
      let mut text = TextLayout::new();
      builder.build_into(&mut text, &node.label);
      text.break_all_lines(Some((NODE_W - 2.0 * PAD_X) as f32));
      self.text_cache.insert(node.id, (node.label.clone(), text));
    }

    let sizes: HashMap<NodeId, layout::Size> = self
      .text_cache
      .iter()
      .map(|(id, (_, text))| (*id, node_size(text)))
      .collect();
    let cfg = LayoutConfig::default();
    let centres = self
      .scene
      .arrangement
      .place(&cfg, |id| sizes.get(&id).copied().unwrap_or(cfg.node_size));
    self.rects = self
      .scene
      .nodes
      .iter()
      .filter_map(|n| {
        let c = centres.get(&n.id)?;
        let size = sizes.get(&n.id)?;
        Some((n.id, Rect::from_center_size((c.x, c.y), (size.w, size.h))))
      })
      .collect();
    self.routes = route_edges(&self.scene.edges, &self.rects);
  }

  /// Fit the whole scene into `viewport`, centred, with a margin. Never
  /// zooms in past 1:1, so a small graph stays readable rather than filling
  /// the window with a couple of giant boxes.
  fn fit_to(&mut self, viewport: masonry::kurbo::Size) {
    let mut union: Option<Rect> = None;
    for &r in self.rects.values() {
      union = Some(match union {
        Some(u) => u.union(r),
        None => r,
      });
    }
    let Some(bounds) = union else { return };
    if viewport.width <= 0.0 || viewport.height <= 0.0 {
      return;
    }

    let margin = 48.0;
    let avail_w = (viewport.width - 2.0 * margin).max(1.0);
    let avail_h = (viewport.height - 2.0 * margin).max(1.0);
    let zoom = (avail_w / bounds.width().max(1.0))
      .min(avail_h / bounds.height().max(1.0))
      .min(1.0)
      .clamp(ZOOM_MIN, ZOOM_MAX);

    self.zoom = zoom;
    // A fit replaces the view outright, so drop any zoom still in flight.
    self.zoom_target = zoom;
    let center = bounds.center();
    // Place the content centre at the viewport centre.
    self.pan = Vec2::new(viewport.width / 2.0, viewport.height / 2.0)
      - zoom * Vec2::new(center.x, center.y);
  }

  /// Request that the next paint refit the graph.
  fn request_fit(&mut self) { self.needs_fit = true; }

  /// Zoom about a screen anchor, keeping the world point under it fixed.
  fn zoom_about(&mut self, anchor: Point, factor: f64) {
    let world = self.to_world(anchor);
    self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    // Solve pan so that transform(world) == anchor again.
    self.pan =
      Vec2::new(anchor.x, anchor.y) - self.zoom * Vec2::new(world.x, world.y);
  }
}

/// Pointer position of an event, in physical pixels, converted to
/// widget-local logical coordinates.
fn local_pos(
  ctx: &EventCtx<'_>,
  physical: masonry::dpi::PhysicalPosition<f64>,
) -> Point {
  ctx.local_position(physical)
}

impl Widget for CanvasWidget {
  type Action = CanvasAction;

  fn on_pointer_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &PointerEvent,
  ) {
    match event {
      PointerEvent::Down(e) => {
        let p = local_pos(ctx, e.state.position);
        ctx.capture_pointer();
        self.press_origin = Some(p);
        self.last_pointer = Some(p);
        self.panned = false;
      }
      PointerEvent::Move(u) => {
        let p = local_pos(ctx, u.current.position);
        // Only pan while a press is in progress (tracked ourselves, so this
        // does not depend on pointer capture actually being granted).
        if let (Some(last), Some(origin)) =
          (self.last_pointer, self.press_origin)
        {
          if origin.distance(p) > CLICK_SLOP {
            self.panned = true;
          }
          if self.panned {
            self.pan += p - last;
            ctx.request_render();
          }
          self.last_pointer = Some(p);
        }
      }
      PointerEvent::Up(e) => {
        // A gesture that started on the canvas and never panned is a click:
        // (de)select. We rely on our own press tracking rather than
        // `is_active()` so a click still registers even if pointer capture
        // was not granted.
        if self.press_origin.take().is_some() {
          let p = local_pos(ctx, e.state.position);
          if !self.panned && e.button == Some(PointerButton::Primary) {
            let hit = self.hit_test(self.to_world(p));
            ctx.submit_action::<CanvasAction>(CanvasAction::Select(hit));
          }
        }
        self.last_pointer = None;
        self.panned = false;
      }
      PointerEvent::Scroll(s) => {
        let p = local_pos(ctx, s.state.position);
        let dy = match s.delta {
          masonry::core::ScrollDelta::LineDelta(_, y) => y as f64,
          masonry::core::ScrollDelta::PixelDelta(pos) => pos.y / 40.0,
          _ => 0.0,
        };
        if dy != 0.0 {
          // Scroll up (positive) zooms in. Only the target moves here; the
          // animation frames ease the view after it.
          self.zoom_target =
            (self.zoom_target * (dy * 0.1).exp()).clamp(ZOOM_MIN, ZOOM_MAX);
          self.zoom_anchor = p;
          ctx.request_anim_frame();
        }
      }
      _ => {}
    }
  }

  fn on_anim_frame(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    interval: u64,
  ) {
    // Ease in log space, so zooming in and out feel the same speed. A long
    // stall (the first frame, or a hitch) is capped so it cannot overshoot.
    let dt = (interval as f64 / 1e9).min(0.1);
    let gap = (self.zoom_target / self.zoom).ln();
    let step = if gap.abs() < 1e-3 {
      gap
    } else {
      gap * (1.0 - (-ZOOM_RATE * dt).exp())
    };
    self.zoom_about(self.zoom_anchor, step.exp());
    ctx.request_render();
    if (self.zoom_target / self.zoom).ln().abs() >= 1e-3 {
      ctx.request_anim_frame();
    }
  }

  fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

  fn layout(
    &mut self,
    ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    bc: &BoxConstraints,
  ) -> masonry::kurbo::Size {
    // Cached layouts were shaped against the old font set (masonry asks that
    // this be checked in the layout pass, not at paint time).
    if ctx.fonts_changed() {
      self.text_cache.clear();
    }
    // The canvas is the one widget of ours with a layout pass, so it is where
    // the default family gets remapped. The bundled face only appears once
    // the driver registers it, which flags `fonts_changed` for this same pass
    // — so the text inputs laid out after us reshape in the right face.
    if !self.font_ready {
      let (font_cx, _) = ctx.text_contexts();
      self.font_ready = font::install(font_cx);
    }
    self.measure(ctx);
    // Fill whatever the parent offers; fall back to a sane size if
    // unconstrained.
    let max = bc.max();
    let w = if max.width.is_finite() {
      max.width
    } else {
      800.0
    };
    let h = if max.height.is_finite() {
      max.height
    } else {
      600.0
    };
    let size = bc.constrain((w, h));
    // A resize leaves the graph off-centre, so refit on the next paint.
    if self.last_size.is_some_and(|prev| prev != size) {
      self.request_fit();
    }
    self.last_size = Some(size);
    size
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let size = ctx.size();
    // Fit-to-view once the viewport size is known (startup + on request).
    if self.needs_fit && !self.scene.nodes.is_empty() {
      self.fit_to(size);
      self.needs_fit = false;
    }
    let tf = self.transform();

    // Background.
    scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(self.theme.bg),
      None,
      &Rect::from_origin_size((0.0, 0.0), (size.width, size.height)),
    );

    // Edges under nodes.
    for (edge, route) in self.scene.edges.iter().zip(&self.routes) {
      if let Some(route) = *route {
        paint_edge(scene, tf, edge, route, self.theme);
      }
    }

    // Nodes on top.
    for node in &self.scene.nodes {
      if let Some(&rect) = self.rects.get(&node.id) {
        self.paint_node(scene, tf, node, rect);
      }
    }
  }

  fn accessibility_role(&self) -> Role { Role::GenericContainer }

  fn accessibility(
    &mut self,
    _ctx: &mut AccessCtx<'_>,
    _props: &PropertiesRef<'_>,
    _node: &mut AccessNode,
  ) {
  }

  fn children_ids(&self) -> ChildrenIds { ChildrenIds::new() }
}

impl CanvasWidget {
  /// Paint a single node: shape, fill, border and label text.
  fn paint_node(
    &self,
    scene: &mut Scene,
    tf: Affine,
    node: &RenderNode,
    rect: Rect,
  ) {
    let (fill, border) = self.theme.for_state(node.state);
    let fill = if node.dimmed { theme::dim(fill) } else { fill };

    match node.kind {
      NodeKind::Task { .. } => {
        let shape = RoundedRect::from_rect(rect, 8.0);
        scene.fill(Fill::NonZero, tf, &Brush::Solid(fill), None, &shape);
        stroke(scene, tf, &shape, border, node.selected, self.theme);
      }
      NodeKind::Condition { .. } => {
        let shape = chamfered(rect);
        scene.fill(Fill::NonZero, tf, &Brush::Solid(fill), None, &shape);
        stroke(scene, tf, &shape, border, node.selected, self.theme);
      }
    }

    // Labels are shaped in the layout pass; one is only missing if this
    // paint raced a scene change, and the next frame will have it.
    if let Some((_, text)) = self.text_cache.get(&node.id) {
      // Centred vertically, so a box held open at `MIN_LINES` does not
      // leave a one-line label stuck to its top.
      let text_h = text.height() as f64;
      let origin = Point::new(rect.x0 + PAD_X, rect.center().y - text_h / 2.0);
      render_text(
        scene,
        tf * Affine::translate(origin.to_vec2()),
        text,
        &[Brush::Solid(self.theme.text)],
        true,
      );
    }
  }
}

/// A node's box size for its shaped label: the fixed width, and tall enough
/// for every line of the label but never fewer than [`MIN_LINES`].
fn node_size(text: &TextLayout<BrushIndex>) -> layout::Size {
  let min_text = MIN_LINES * f64::from(LABEL_SIZE * LABEL_LINE);
  layout::Size {
    w: NODE_W,
    h: (text.height() as f64).max(min_text) + 2.0 * PAD_Y,
  }
}

/// How far a condition's corners are cut back, in world units.
const CHAMFER: f64 = 10.0;

/// Build a condition's outline: `rect` with its corners cut off at 45°.
///
/// A diamond said "condition" clearly but wasted half its area, so a label
/// had to fit in the middle third; the chamfer keeps the cue while leaving
/// the text the same room a task gets.
fn chamfered(rect: Rect) -> BezPath {
  let c = CHAMFER.min(rect.width() / 2.0).min(rect.height() / 2.0);
  let mut p = BezPath::new();
  p.move_to((rect.x0 + c, rect.y0));
  p.line_to((rect.x1 - c, rect.y0));
  p.line_to((rect.x1, rect.y0 + c));
  p.line_to((rect.x1, rect.y1 - c));
  p.line_to((rect.x1 - c, rect.y1));
  p.line_to((rect.x0 + c, rect.y1));
  p.line_to((rect.x0, rect.y1 - c));
  p.line_to((rect.x0, rect.y0 + c));
  p.close_path();
  p
}

/// Stroke a shape's border, thicker and accented when selected.
fn stroke(
  scene: &mut Scene,
  tf: Affine,
  shape: &impl masonry::kurbo::Shape,
  border: Color,
  selected: bool,
  theme: &Theme,
) {
  let (color, width) = if selected {
    (theme.accent, 3.0)
  } else {
    (border, 1.5)
  };
  scene.stroke(
    &masonry::kurbo::Stroke::new(width),
    tf,
    &Brush::Solid(color),
    None,
    shape,
  );
}

/// Arrowhead length and half-width, in world units.
const HEAD_LEN: f64 = 9.0;
const HEAD_HALF_W: f64 = 4.5;
/// Space left between an arrowhead's tip and the node it points at.
const TIP_GAP: f64 = 2.0;
/// Spacing between edge endpoints that share one side of a node.
const PORT_PITCH: f64 = 16.0;
/// Fraction of a side's length that endpoints may spread across.
const PORT_SPAN: f64 = 0.7;

/// Where an edge leaves its dependent and enters its requirement, and which
/// way it travels there.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Route {
  /// On the border of the `from` node.
  start: Point,
  /// On the border of the `to` node, where the arrowhead's tip goes.
  end:   Point,
  /// Unit direction the edge leaves `start` and arrives at `end` along:
  /// down or up between rows, sideways within one.
  axis:  Vec2,
}

/// The side of a box an edge attaches to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
  Top,
  Bottom,
  Left,
  Right,
}

/// Route every edge between the boxes in `rects`, index-aligned with
/// `edges` (`None` where an end is not placed).
///
/// Edges run bottom-to-top between rows — leaving the lower side of the
/// node above and entering the upper side of the node below, whichever way
/// the arrow points — and side-to-side within a row. Where several edges
/// share one side of a node, their endpoints are spread along it in the
/// order of their far ends, so they fan out instead of converging on a
/// single point and their curves do not cross at the node.
fn route_edges(
  edges: &[RenderEdge],
  rects: &HashMap<NodeId, Rect>,
) -> Vec<Option<Route>> {
  // First pass: sides and axes, plus who attaches where.
  let mut sides: Vec<Option<(Side, Side, Vec2)>> = Vec::new();
  let mut ports: HashMap<(NodeId, Side), Vec<(f64, usize, bool)>> =
    HashMap::new();
  for (i, edge) in edges.iter().enumerate() {
    let (Some(a), Some(b)) = (rects.get(&edge.from), rects.get(&edge.to))
    else {
      sides.push(None);
      continue;
    };
    let (from_side, to_side, axis) = if b.y0 >= a.y1 {
      (Side::Bottom, Side::Top, Vec2::new(0.0, 1.0))
    } else if b.y1 <= a.y0 {
      (Side::Top, Side::Bottom, Vec2::new(0.0, -1.0))
    } else if b.center().x >= a.center().x {
      (Side::Right, Side::Left, Vec2::new(1.0, 0.0))
    } else {
      (Side::Left, Side::Right, Vec2::new(-1.0, 0.0))
    };
    // Ports along a side are ordered by where the other end lies along it.
    let along = |r: &Rect| {
      if axis.x == 0.0 {
        r.center().x
      } else {
        r.center().y
      }
    };
    ports
      .entry((edge.from, from_side))
      .or_default()
      .push((along(b), i, true));
    ports
      .entry((edge.to, to_side))
      .or_default()
      .push((along(a), i, false));
    sides.push(Some((from_side, to_side, axis)));
  }

  let mut starts: Vec<Option<Point>> = vec![None; edges.len()];
  let mut ends: Vec<Option<Point>> = vec![None; edges.len()];
  for ((node, side), mut list) in ports {
    let rect = rects[&node];
    list.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    let n = list.len();
    for (k, &(_, i, is_start)) in list.iter().enumerate() {
      let p = port(rect, side, k, n);
      if is_start {
        starts[i] = Some(p);
      } else {
        ends[i] = Some(p);
      }
    }
  }

  sides
    .iter()
    .enumerate()
    .map(|(i, s)| {
      let (_, _, axis) = (*s)?;
      Some(Route {
        start: starts[i]?,
        end: ends[i]?,
        axis,
      })
    })
    .collect()
}

/// The `k`th of `n` evenly spaced endpoints on `side` of `rect`, centred on
/// the side and never spreading past [`PORT_SPAN`] of it.
fn port(rect: Rect, side: Side, k: usize, n: usize) -> Point {
  let len = match side {
    Side::Top | Side::Bottom => rect.width(),
    Side::Left | Side::Right => rect.height(),
  };
  let span = (PORT_PITCH * n.saturating_sub(1) as f64).min(len * PORT_SPAN);
  let offset = if n > 1 {
    -span / 2.0 + span * k as f64 / (n - 1) as f64
  } else {
    0.0
  };
  let c = rect.center();
  match side {
    Side::Top => Point::new(c.x + offset, rect.y0),
    Side::Bottom => Point::new(c.x + offset, rect.y1),
    Side::Left => Point::new(rect.x0, c.y + offset),
    Side::Right => Point::new(rect.x1, c.y + offset),
  }
}

/// The curve an edge is drawn along: a cubic that leaves `start` and meets
/// the arrowhead's base both square to their node, so edges bend smoothly
/// between rows the way Mermaid draws them. Returns the curve and the tip.
fn edge_curve(route: Route) -> (BezPath, Point) {
  let Route { start, end, axis } = route;
  let tip = end - axis * TIP_GAP;
  let base = tip - axis * HEAD_LEN;
  // Handles reach halfway along the travel axis, with a floor so a short
  // hop between close rows still bends rather than kinking.
  let reach = ((base - start).dot(axis).abs() / 2.0).max(24.0);
  let mut path = BezPath::new();
  path.move_to(start);
  path.curve_to(start + axis * reach, base - axis * reach, base);
  (path, tip)
}

/// Paint one edge as a curve plus an arrowhead at the requirement end.
fn paint_edge(
  scene: &mut Scene,
  tf: Affine,
  edge: &RenderEdge,
  route: Route,
  theme: &Theme,
) {
  let color = if edge.reversed {
    theme.cycle
  } else {
    theme.edge
  };
  let stroke_style = match edge.kind {
    EdgeKind::Dependency => masonry::kurbo::Stroke::new(1.5),
    _ => masonry::kurbo::Stroke::new(1.5),
  };
  let (curve, tip) = edge_curve(route);
  scene.stroke(&stroke_style, tf, &Brush::Solid(color), None, &curve);

  // Arrowhead along the travel axis: the curve arrives square to the node,
  // so the head lines up with it exactly.
  let axis = route.axis;
  let base = tip - axis * HEAD_LEN;
  let perp = Vec2::new(-axis.y, axis.x) * HEAD_HALF_W;
  let mut head = BezPath::new();
  head.move_to(tip);
  head.line_to(base + perp);
  head.line_to(base - perp);
  head.close_path();
  scene.fill(Fill::NonZero, tf, &Brush::Solid(color), None, &head);
}

// --- the view -----------------------------------------------------------

/// A Xilem [`View`] hosting the [`CanvasWidget`]. Rebuilds push the latest
/// [`CanvasScene`] into the widget; the widget's [`CanvasAction`]s are routed
/// to `on_action`.
pub struct Canvas<F> {
  scene:     CanvasScene,
  /// The palette the widget paints with.
  theme:     &'static Theme,
  /// A monotonically increasing token; whenever it changes, the widget
  /// refits the graph into the viewport (drives the "Recenter" button).
  fit_epoch: u64,
  on_action: F,
}

/// Construct a canvas view from a scene, a fit epoch, and an action handler.
/// Bump `fit_epoch` to recentre the view on the next rebuild.
///
/// The handler's return value is wrapped in [`MessageResult::Action`], which
/// is what tells the Xilem driver to re-run `app_logic` against the mutated
/// app state. (`RequestRebuild` only re-diffs the *existing* view tree, so a
/// selection change made here would never reach the widgets.)
pub fn canvas<State, Action, F>(
  scene: CanvasScene,
  theme: &'static Theme,
  fit_epoch: u64,
  on_action: F,
) -> Canvas<impl Fn(&mut State, CanvasAction) -> MessageResult<Action>>
where
  F: Fn(&mut State, CanvasAction) -> Action + 'static,
{
  Canvas {
    scene,
    theme,
    fit_epoch,
    on_action: move |state: &mut State, action| {
      MessageResult::Action(on_action(state, action))
    },
  }
}

impl<F> ViewMarker for Canvas<F> {}

impl<F, State, Action> View<State, Action, ViewCtx> for Canvas<F>
where
  F: Fn(&mut State, CanvasAction) -> MessageResult<Action> + 'static,
  State: 'static,
  Action: 'static,
{
  type Element = Pod<CanvasWidget>;
  /// The last fit epoch we applied, so we only refit when it changes.
  type ViewState = u64;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    _app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let pod = ctx.with_action_widget(|ctx| {
      let mut w = CanvasWidget::new(self.theme);
      w.set_scene(self.scene.clone());
      ctx.create_pod(w)
    });
    (pod, self.fit_epoch)
  }

  fn rebuild(
    &self,
    _prev: &Self,
    last_epoch: &mut Self::ViewState,
    _ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    _app_state: &mut State,
  ) {
    // The scene is cheap to diff by clone-and-replace for v1; a later pass
    // can compare against `_prev.scene`.
    element.widget.set_scene(self.scene.clone());
    element.widget.set_theme(self.theme);
    // Labels may have changed, and with them node sizes and placement.
    element.ctx.request_layout();
    if *last_epoch != self.fit_epoch {
      element.widget.request_fit();
      *last_epoch = self.fit_epoch;
    }
    element.ctx.request_render();
  }

  fn teardown(
    &self,
    _view_state: &mut Self::ViewState,
    ctx: &mut ViewCtx,
    element: Mut<'_, Self::Element>,
  ) {
    ctx.teardown_leaf(element);
  }

  fn message(
    &self,
    _view_state: &mut Self::ViewState,
    message: &mut MessageContext,
    _element: Mut<'_, Self::Element>,
    app_state: &mut State,
  ) -> MessageResult<Action> {
    match message.take_message::<CanvasAction>() {
      Some(action) => (self.on_action)(app_state, *action),
      None => MessageResult::Stale,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn edge(from: u128, to: u128) -> RenderEdge {
    RenderEdge {
      from:     NodeId::from_u128(from),
      to:       NodeId::from_u128(to),
      kind:     EdgeKind::Dependency,
      reversed: false,
    }
  }

  fn boxes(list: &[(u128, Rect)]) -> HashMap<NodeId, Rect> {
    list
      .iter()
      .map(|(id, r)| (NodeId::from_u128(*id), *r))
      .collect()
  }

  /// Between rows, an edge leaves the bottom of the upper node and enters
  /// the top of the lower one — whichever way the arrow points, so a
  /// reversed cycle edge climbs back up rather than cutting through boxes.
  #[test]
  fn edges_attach_to_the_facing_sides() {
    let upper = Rect::new(0.0, 0.0, 150.0, 50.0);
    let lower = Rect::new(0.0, 100.0, 150.0, 150.0);
    let rects = boxes(&[(1, upper), (2, lower)]);

    let down = route_edges(&[edge(1, 2)], &rects)[0].unwrap();
    assert_eq!(down.start, Point::new(75.0, 50.0));
    assert_eq!(down.end, Point::new(75.0, 100.0));
    assert_eq!(down.axis, Vec2::new(0.0, 1.0));

    let up = route_edges(&[edge(2, 1)], &rects)[0].unwrap();
    assert_eq!(up.start, Point::new(75.0, 100.0));
    assert_eq!(up.end, Point::new(75.0, 50.0));
    assert_eq!(up.axis, Vec2::new(0.0, -1.0));
  }

  /// Edges sharing a side fan out in the order of their far ends, so they
  /// neither converge on one point nor cross at the node.
  #[test]
  fn shared_sides_spread_endpoints_in_order() {
    let parent = Rect::new(100.0, 0.0, 250.0, 50.0);
    let left = Rect::new(0.0, 100.0, 150.0, 150.0);
    let right = Rect::new(200.0, 100.0, 350.0, 150.0);
    let rects = boxes(&[(1, parent), (2, left), (3, right)]);
    // Listed right-first, to show order comes from geometry.
    let routes = route_edges(&[edge(1, 3), edge(1, 2)], &rects);
    let (to_right, to_left) = (routes[0].unwrap(), routes[1].unwrap());
    assert!(to_left.start.x < to_right.start.x);
    assert_eq!(to_right.start.x - to_left.start.x, PORT_PITCH);
    // Centred on the side as a group.
    assert_eq!((to_left.start.x + to_right.start.x) / 2.0, 175.0);
  }

  /// However many edges share a side, they stay within its middle span.
  #[test]
  fn ports_never_leave_the_side() {
    let r = Rect::new(0.0, 0.0, 150.0, 50.0);
    for n in 1..30 {
      for k in 0..n {
        let p = port(r, Side::Top, k, n);
        assert!(p.x >= 75.0 - 75.0 * PORT_SPAN - 1e-9);
        assert!(p.x <= 75.0 + 75.0 * PORT_SPAN + 1e-9);
        assert_eq!(p.y, 0.0);
      }
    }
  }

  /// The curve leaves and arrives square to its nodes, and stops short of
  /// the tip by the arrowhead's length so the head caps it cleanly.
  #[test]
  fn curve_meets_the_arrowhead_square_on() {
    let route = Route {
      start: Point::new(0.0, 0.0),
      end:   Point::new(80.0, 100.0),
      axis:  Vec2::new(0.0, 1.0),
    };
    let (curve, tip) = edge_curve(route);
    assert_eq!(tip, Point::new(80.0, 100.0 - TIP_GAP));
    let els = curve.elements();
    let masonry::kurbo::PathEl::CurveTo(c1, c2, base) = els[1] else {
      panic!("expected a cubic, got {els:?}");
    };
    assert_eq!(c1.x, 0.0, "leaves straight down");
    assert_eq!(c2.x, base.x, "arrives straight down");
    assert_eq!(base, Point::new(80.0, tip.y - HEAD_LEN));
  }
}
