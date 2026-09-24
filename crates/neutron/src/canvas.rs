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

use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
};

use base::{EdgeId, NodeId, NodeKind, NodeState};
use layout::{Arrangement, Channel, LayoutConfig};
use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, CursorIcon, EventCtx,
    LayoutCtx, PaintCtx, PointerButton, PointerEvent, PropertiesMut,
    PropertiesRef, QueryCtx, RegisterCtx, StyleProperty, UpdateCtx, Widget,
    render_text,
  },
  kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Size, Stroke, Vec2},
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
/// Screen pixels kept clear around the graph when fitting it, and around a
/// node revealed by [`CameraRequest::Reveal`].
const VIEW_MARGIN: f64 = 48.0;
/// Screen pixels beyond the viewport that still count as on screen when
/// culling, so a selection outline or an edge's stroke that pokes past a
/// box is never cut off at the window edge.
const CULL_MARGIN: f64 = 16.0;

// --- render data --------------------------------------------------------

/// One node as the canvas needs to draw and hit-test it.
#[derive(Clone, Debug)]
pub struct RenderNode {
  /// Which node this is (returned in [`CanvasAction::Click`]).
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
  /// Which edge this is, to find the channels it was given.
  pub id:       EdgeId,
  /// The dependent end.
  pub from:     NodeId,
  /// The requirement end (the arrow points here).
  pub to:       NodeId,
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

/// How much of the canvas the chrome floating over it covers on each side,
/// in logical pixels. Fitting and revealing aim at the area left uncovered.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
  /// Covered from the top edge.
  pub top:    f64,
  /// Covered from the right edge.
  pub right:  f64,
  /// Covered from the bottom edge.
  pub bottom: f64,
  /// Covered from the left edge.
  pub left:   f64,
}

/// Something the app asks the canvas's camera to do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraRequest {
  /// Fit the whole graph into the uncovered area.
  Fit,
  /// Pan, easing, until the node is inside the uncovered area. A node that
  /// is already comfortably in view does not move.
  Reveal(NodeId),
  /// Step the zoom, easing, about the centre of the uncovered area.
  Zoom(ZoomStep),
}

/// A zoom step from the zoom controls or keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoomStep {
  /// One step closer.
  In,
  /// One step further out.
  Out,
  /// Back to 100%.
  Reset,
}

/// How much one [`ZoomStep`] multiplies or divides the zoom by.
const ZOOM_STEP: f64 = 1.25;

/// The latest camera request, tagged with a counter the app bumps for each
/// new one, so repeating the same request (Fit twice) still acts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
  /// Bumped per request.
  pub epoch:   u64,
  /// What to do.
  pub request: CameraRequest,
}

/// Link mode, as the canvas shows it: which node is gaining requirements,
/// which nodes a click cannot add, and which would close a cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkMode {
  /// The node gaining requirements (the selection).
  pub source:       NodeId,
  /// Its name, for the banner.
  pub name:         String,
  /// The source itself and the nodes it already requires: dimmed, and a
  /// click on them adds nothing.
  pub taken:        HashSet<NodeId>,
  /// Nodes that already require the source, so requiring them would close a
  /// cycle: allowed, but outlined as a warning.
  pub closes_cycle: HashSet<NodeId>,
}

/// Something the user did on the canvas that the app must react to.
#[derive(Clone, Debug)]
pub enum CanvasAction {
  /// A click on a node, or on empty space (`None`), with Shift held or not.
  Click {
    /// The node clicked, if any.
    node:  Option<NodeId>,
    /// Whether Shift was held.
    shift: bool,
  },
  /// The zoom level, as a whole percentage, changed.
  Zoomed(u32),
}

// --- the widget ---------------------------------------------------------

/// The Masonry canvas widget. Owns the view transform (pan/zoom) and the
/// interaction state; the scene is pushed in from the view.
pub struct CanvasWidget {
  scene:         Arc<CanvasScene>,
  /// Whether labels, sizes, placement and routes need recomputing in the
  /// next layout pass: set by a new scene or a font change, and cleared
  /// once done, so resizes and repaints do not redo them.
  dirty:         bool,
  /// World→screen translation.
  pan:           Vec2,
  /// World→screen scale.
  zoom:          f64,
  /// The scale the view is animating towards; wheel input moves this, and
  /// each animation frame eases [`Self::zoom`] after it.
  zoom_target:   f64,
  /// Screen point the animated zoom is anchored on (the last wheel position).
  zoom_anchor:   Point,
  /// The translation an eased pan is heading for, if one is in flight.
  pan_target:    Option<Vec2>,
  /// A node to reveal once it has been placed (the request can arrive in the
  /// same rebuild as the scene that adds it).
  reveal:        Option<NodeId>,
  /// How much of the canvas the chrome covers.
  insets:        Insets,
  /// Link mode, while it is armed.
  link:          Option<LinkMode>,
  /// The node under the pointer, tracked while no button is held.
  hover:         Option<NodeId>,
  /// The zoom percentage last reported to the app, so it hears of changes
  /// only.
  reported_zoom: u32,
  /// Pointer position at the last `Down`, for click-vs-pan discrimination.
  press_origin:  Option<Point>,
  /// Previous pointer position while dragging, for incremental panning.
  last_pointer:  Option<Point>,
  /// Whether the current gesture has moved far enough to be a pan.
  panned:        bool,
  /// When set, the next paint fits the whole graph into the viewport. Set on
  /// construction (so the app opens centred) and whenever a recenter is
  /// requested (PLAN §5, §6.3).
  needs_fit:     bool,
  /// Viewport size from the previous layout pass, so a resize can request
  /// a refit (the existing fit is centred on the old viewport).
  last_size:     Option<Size>,
  /// The palette every colour painted here comes from, pushed in from the
  /// view on rebuild.
  theme:         &'static Theme,
  /// Cached per-node text layouts, keyed by id, invalidated when the label
  /// text changes.
  text_cache:    HashMap<NodeId, (String, TextLayout<BrushIndex>)>,
  /// Every node's box in world coordinates, placed in the layout pass from
  /// the scene's arrangement and the measured labels.
  rects:         HashMap<NodeId, Rect>,
  /// Where each of the scene's edges attaches, index-aligned with
  /// `scene.edges`; routed alongside `rects`.
  routes:        Vec<Option<Route>>,
}

impl CanvasWidget {
  /// A fresh canvas with an identity transform and no scene.
  pub fn new(theme: &'static Theme) -> Self {
    Self {
      scene: Arc::default(),
      dirty: true,
      pan: Vec2::new(60.0, 60.0),
      zoom: 1.0,
      zoom_target: 1.0,
      zoom_anchor: Point::ORIGIN,
      pan_target: None,
      reveal: None,
      insets: Insets::default(),
      reported_zoom: 100,
      link: None,
      hover: None,
      press_origin: None,
      last_pointer: None,
      panned: false,
      needs_fit: true,
      last_size: None,
      theme,
      text_cache: HashMap::new(),
      rects: HashMap::new(),
      routes: Vec::new(),
    }
  }

  /// Replace the scene. The caller must request a layout, which re-measures
  /// and re-places the nodes.
  fn set_scene(&mut self, scene: Arc<CanvasScene>) {
    // Drop cached text for nodes that vanished.
    let live: std::collections::HashSet<NodeId> =
      scene.nodes.iter().map(|n| n.id).collect();
    self.text_cache.retain(|id, _| live.contains(id));
    self.scene = scene;
    self.dirty = true;
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
    let placed = self
      .scene
      .arrangement
      .place(&cfg, |id| sizes.get(&id).copied().unwrap_or(cfg.node_size));
    let centres = &placed.nodes;
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
    self.routes = route_edges(&self.scene.edges, &self.rects, &placed.channels);
  }

  /// Fit the whole scene into the part of `viewport` the chrome leaves
  /// uncovered.
  fn fit_to(&mut self, viewport: Size) {
    let Some(bounds) = self.rects.values().copied().reduce(|a, b| a.union(b))
    else {
      return;
    };
    let Some((zoom, pan)) = fit(bounds, uncovered(viewport, self.insets))
    else {
      return;
    };
    self.zoom = zoom;
    self.pan = pan;
    // A fit replaces the view outright, so drop any motion still in flight.
    self.zoom_target = zoom;
    self.pan_target = None;
  }

  /// Act on a camera request from the app.
  fn apply(&mut self, request: CameraRequest) {
    match request {
      CameraRequest::Fit => self.needs_fit = true,
      CameraRequest::Reveal(node) => self.reveal = Some(node),
      CameraRequest::Zoom(step) => {
        self.zoom_target = match step {
          ZoomStep::In => self.zoom_target * ZOOM_STEP,
          ZoomStep::Out => self.zoom_target / ZOOM_STEP,
          ZoomStep::Reset => 1.0,
        }
        .clamp(ZOOM_MIN, ZOOM_MAX);
        let size = self.last_size.unwrap_or_default();
        self.zoom_anchor = uncovered(size, self.insets).center();
        self.pan_target = None;
      }
    }
  }

  /// The zoom level as a whole percentage, if it differs from the one last
  /// reported (and records it as reported).
  fn zoom_change(&mut self) -> Option<u32> {
    let percent = (self.zoom * 100.0).round() as u32;
    (percent != self.reported_zoom).then(|| {
      self.reported_zoom = percent;
      percent
    })
  }

  /// Start panning towards a pending [`Self::reveal`], if its node is placed
  /// and not already in view. Returns whether it resolved the request.
  fn start_reveal(&mut self) -> bool {
    let (Some(node), Some(size)) = (self.reveal, self.last_size) else {
      return false;
    };
    let Some(&rect) = self.rects.get(&node) else {
      return false;
    };
    self.reveal = None;
    let view = uncovered(size, self.insets);
    self.pan_target = reveal_pan(rect, self.zoom, self.pan, view);
    true
  }

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
            self.pan_target = None;
            self.pan += p - last;
            ctx.request_render();
          }
          self.last_pointer = Some(p);
        } else {
          // Hovering: in link mode, the node under the pointer gets a
          // preview of the edge a click would add.
          let hover = self.hit_test(self.to_world(p));
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
        // (de)select. We rely on our own press tracking rather than
        // `is_active()` so a click still registers even if pointer capture
        // was not granted.
        if self.press_origin.take().is_some() {
          let p = local_pos(ctx, e.state.position);
          if !self.panned && e.button == Some(PointerButton::Primary) {
            let hit = self.hit_test(self.to_world(p));
            ctx.submit_action::<CanvasAction>(CanvasAction::Click {
              node:  hit,
              shift: e.state.modifiers.shift(),
            });
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
          self.pan_target = None;
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
    let ease = 1.0 - (-ZOOM_RATE * dt).exp();
    self.start_reveal();

    let gap = (self.zoom_target / self.zoom).ln();
    let step = if gap.abs() < 1e-3 { gap } else { gap * ease };
    self.zoom_about(self.zoom_anchor, step.exp());
    let mut moving = (self.zoom_target / self.zoom).ln().abs() >= 1e-3;

    // Pans ease the same way, in screen pixels.
    if let Some(target) = self.pan_target {
      let gap = target - self.pan;
      if gap.hypot() < 0.5 {
        self.pan = target;
        self.pan_target = None;
      } else {
        self.pan += gap * ease;
        moving = true;
      }
    }

    if let Some(percent) = self.zoom_change() {
      ctx.submit_action::<CanvasAction>(CanvasAction::Zoomed(percent));
    }
    ctx.request_render();
    if moving || self.reveal.is_some() {
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
      self.dirty = true;
    }
    if self.dirty {
      self.measure(ctx);
      self.dirty = false;
    }
    let size = canvas_size(bc);
    // A resize leaves the graph off-centre, so refit.
    if self.last_size.is_some_and(|prev| prev != size) {
      self.needs_fit = true;
    }
    self.last_size = Some(size);
    // Fit once the viewport size and the node boxes are both known: at
    // startup, after a resize, and on request.
    if self.needs_fit && !self.rects.is_empty() {
      self.fit_to(size);
      self.needs_fit = false;
      if let Some(percent) = self.zoom_change() {
        ctx.submit_action::<CanvasAction>(CanvasAction::Zoomed(percent));
      }
    }
    size
  }

  fn paint(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    _props: &PropertiesRef<'_>,
    scene: &mut Scene,
  ) {
    let size = ctx.size();
    let tf = self.transform();
    let view = visible_world(tf, size);

    // Background.
    scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(self.theme.bg),
      None,
      &Rect::from_origin_size((0.0, 0.0), (size.width, size.height)),
    );

    // Edges under nodes. Only what can be seen is encoded: at scale most of
    // a big graph is off screen, and each label is a text run to render.
    for (edge, route) in self.scene.edges.iter().zip(&self.routes) {
      if let Some(route) = route
        && route.bounds().overlaps(view)
      {
        paint_edge(scene, tf, edge, route, self.theme);
      }
    }

    // In link mode, the edge a click on the hovered node would add.
    if let Some(link) = &self.link
      && let Some(target) = self.hover
      && !link.taken.contains(&target)
      && let (Some(&from), Some(&to)) =
        (self.rects.get(&link.source), self.rects.get(&target))
    {
      let color = if link.closes_cycle.contains(&target) {
        self.theme.cycle
      } else {
        self.theme.accent
      };
      paint_preview(scene, tf, from, to, color);
    }

    // Nodes on top.
    for node in &self.scene.nodes {
      if let Some(&rect) = self.rects.get(&node.id)
        && rect.overlaps(view)
      {
        self.paint_node(scene, tf, node, rect);
      }
    }
  }

  fn get_cursor(&self, _ctx: &QueryCtx<'_>, _pos: Point) -> CursorIcon {
    if self.link.is_some() {
      CursorIcon::Crosshair
    } else {
      CursorIcon::Default
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
    // In link mode, nodes a click cannot add fade back.
    let taken = self
      .link
      .as_ref()
      .is_some_and(|l| l.taken.contains(&node.id));
    let fill = if node.dimmed || taken {
      theme::dim(fill)
    } else {
      fill
    };
    let border = if taken { theme::dim(border) } else { border };

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

    // In link mode, a dashed warning ring round nodes that would close a
    // cycle if required.
    if self
      .link
      .as_ref()
      .is_some_and(|l| l.closes_cycle.contains(&node.id))
    {
      let ring = RoundedRect::from_rect(rect.inflate(4.0, 4.0), 11.0);
      scene.stroke(
        &Stroke::new(2.0).with_dashes(0.0, [6.0, 4.0]),
        tf,
        &Brush::Solid(self.theme.cycle),
        None,
        &ring,
      );
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
/// Shortest handle on an edge's curves, so a hop between close rows still
/// bends rather than kinking.
const MIN_REACH: f64 = 24.0;

/// Where an edge leaves its dependent and enters its requirement, and which
/// way it travels there.
#[derive(Clone, Debug, PartialEq)]
struct Route {
  /// On the border of the `from` node.
  start: Point,
  /// On the border of the `to` node, where the arrowhead's tip goes.
  end:   Point,
  /// Unit direction the edge leaves `start` and arrives at `end` along:
  /// down or up between rows, sideways within one.
  axis:  Vec2,
  /// For an edge that skips rows, the straight run it makes through each
  /// skipped row, as `(entry, exit)` in travel order. Following these keeps
  /// the edge in the gap the layout reserved instead of crossing nodes.
  via:   Vec<(Point, Point)>,
}

impl Route {
  /// A box containing everything [`paint_edge`] draws for this route: its
  /// endpoints and channels, grown by how far a curve's handles and the
  /// arrowhead can reach past them.
  fn bounds(&self) -> Rect {
    let mut r = Rect::from_points(self.start, self.end);
    for &(entry, exit) in &self.via {
      r = r.union_pt(entry).union_pt(exit);
    }
    // Handles reach at most `MIN_REACH` past an endpoint along the axis;
    // sideways, the curve stays within its endpoints.
    let pad = MIN_REACH + HEAD_HALF_W;
    r.inflate(pad, pad)
  }
}

/// The side of a box an edge attaches to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
  Top,
  Bottom,
  Left,
  Right,
}

/// Route every edge between the boxes in `rects`, through its `channels` if
/// it skips rows, index-aligned with `edges` (`None` where an end is not
/// placed).
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
  channels: &HashMap<EdgeId, Vec<Channel>>,
) -> Vec<Option<Route>> {
  // First pass: sides, axes and waypoints, plus who attaches where.
  let mut sides: Vec<Option<(Side, Side, Vec2)>> = Vec::new();
  let mut vias: Vec<Vec<(Point, Point)>> = Vec::new();
  let mut ports: HashMap<(NodeId, Side), Vec<(f64, usize, bool)>> =
    HashMap::new();
  for (i, edge) in edges.iter().enumerate() {
    let (Some(a), Some(b)) = (rects.get(&edge.from), rects.get(&edge.to))
    else {
      sides.push(None);
      vias.push(Vec::new());
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
    // Channels come top row first, each entered at the top; an edge
    // travelling up walks them in reverse and enters each at the bottom.
    let mut via: Vec<(Point, Point)> = channels
      .get(&edge.id)
      .into_iter()
      .flatten()
      .map(|c| (Point::new(c.x, c.top), Point::new(c.x, c.bottom)))
      .collect();
    if axis.y < 0.0 {
      via.reverse();
      for (entry, exit) in &mut via {
        std::mem::swap(entry, exit);
      }
    }
    // Ports along a side are ordered by where the edge heads next: its
    // first channel from the start, its last into the end, else the far
    // node.
    let along = |p: Point| if axis.x == 0.0 { p.x } else { p.y };
    let next = via.first().map_or(b.center(), |v| v.0);
    let prev = via.last().map_or(a.center(), |v| v.1);
    ports.entry((edge.from, from_side)).or_default().push((
      along(next),
      i,
      true,
    ));
    ports
      .entry((edge.to, to_side))
      .or_default()
      .push((along(prev), i, false));
    sides.push(Some((from_side, to_side, axis)));
    vias.push(via);
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
        via: vias[i].clone(),
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

/// The curve an edge is drawn along: cubics between rows that leave and
/// arrive square to whatever they join, so edges bend smoothly the way
/// Mermaid draws them, with a straight run down each skipped row's channel.
/// It ends at the arrowhead's base. Returns the curve and the tip.
fn edge_curve(route: &Route) -> (BezPath, Point) {
  let axis = route.axis;
  let tip = route.end - axis * TIP_GAP;
  let base = tip - axis * HEAD_LEN;
  // Handles reach halfway along the travel axis, with a floor so a short
  // hop between close rows still bends rather than kinking.
  let bend = |path: &mut BezPath, from: Point, to: Point| {
    let reach = ((to - from).dot(axis).abs() / 2.0).max(MIN_REACH);
    path.curve_to(from + axis * reach, to - axis * reach, to);
  };
  let mut path = BezPath::new();
  path.move_to(route.start);
  let mut at = route.start;
  for &(entry, exit) in &route.via {
    bend(&mut path, at, entry);
    path.line_to(exit);
    at = exit;
  }
  bend(&mut path, at, base);
  (path, tip)
}

/// Paint the edge link mode would add from `from` to `to`: dashed, in
/// `color`, routed like a real edge (without channels, since it does not
/// exist yet).
fn paint_preview(
  scene: &mut Scene,
  tf: Affine,
  from: Rect,
  to: Rect,
  color: Color,
) {
  let (a, b) = (NodeId::from_u128(0), NodeId::from_u128(1));
  let edge = RenderEdge {
    id:       EdgeId::from_u128(0),
    from:     a,
    to:       b,
    reversed: false,
  };
  let rects = HashMap::from([(a, from), (b, to)]);
  let Some(route) = route_edges(&[edge], &rects, &HashMap::new()).remove(0)
  else {
    return;
  };
  let (curve, tip) = edge_curve(&route);
  scene.stroke(
    &Stroke::new(2.0).with_dashes(0.0, [6.0, 4.0]),
    tf,
    &Brush::Solid(color),
    None,
    &curve,
  );
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

/// Paint one edge as a curve plus an arrowhead at the requirement end.
fn paint_edge(
  scene: &mut Scene,
  tf: Affine,
  edge: &RenderEdge,
  route: &Route,
  theme: &Theme,
) {
  let color = if edge.reversed {
    theme.cycle
  } else {
    theme.edge
  };
  let stroke_style = masonry::kurbo::Stroke::new(1.5);
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

/// The canvas fills whatever its parent offers, falling back to a sane size
/// when unconstrained.
fn canvas_size(bc: &BoxConstraints) -> Size {
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
  bc.constrain((w, h))
}

/// The part of a `viewport`-sized canvas the chrome leaves uncovered, in
/// screen coordinates. Never inverted, however large the insets.
fn uncovered(viewport: Size, insets: Insets) -> Rect {
  let x0 = insets.left.min(viewport.width);
  let y0 = insets.top.min(viewport.height);
  Rect::new(
    x0,
    y0,
    (viewport.width - insets.right).max(x0),
    (viewport.height - insets.bottom).max(y0),
  )
}

/// The zoom and pan that fit world-space `bounds` into screen-space `view`,
/// centred with a [`VIEW_MARGIN`]. Never zooms in past 1:1, so a small graph
/// stays readable rather than filling the window with a few giant boxes.
/// `None` for an empty view.
fn fit(bounds: Rect, view: Rect) -> Option<(f64, Vec2)> {
  if view.width() <= 0.0 || view.height() <= 0.0 {
    return None;
  }
  let avail_w = (view.width() - 2.0 * VIEW_MARGIN).max(1.0);
  let avail_h = (view.height() - 2.0 * VIEW_MARGIN).max(1.0);
  let zoom = (avail_w / bounds.width().max(1.0))
    .min(avail_h / bounds.height().max(1.0))
    .min(1.0)
    .clamp(ZOOM_MIN, ZOOM_MAX);
  let pan = view.center().to_vec2() - zoom * bounds.center().to_vec2();
  Some((zoom, pan))
}

/// The pan that brings world-space `node` into screen-space `view` at
/// `zoom`: `None` if it is already inside `view` with a [`VIEW_MARGIN`] to
/// spare, else the pan that centres it.
fn reveal_pan(node: Rect, zoom: f64, pan: Vec2, view: Rect) -> Option<Vec2> {
  let on_screen = Affine::translate(pan) * Affine::scale(zoom);
  let shown = on_screen.transform_rect_bbox(node);
  let comfortable = view.inset(-VIEW_MARGIN);
  let inside = comfortable.width() > 0.0
    && comfortable.height() > 0.0
    && comfortable.contains(shown.origin())
    && comfortable.contains(Point::new(shown.x1, shown.y1));
  (!inside).then(|| view.center().to_vec2() - zoom * node.center().to_vec2())
}

/// The world-space area the viewport shows under `tf`, plus
/// [`CULL_MARGIN`].
fn visible_world(tf: Affine, size: Size) -> Rect {
  let screen = Rect::from_origin_size(Point::ORIGIN, size)
    .inflate(CULL_MARGIN, CULL_MARGIN);
  tf.inverse().transform_rect_bbox(screen)
}

// --- the view -----------------------------------------------------------

/// A Xilem [`View`] hosting the [`CanvasWidget`]. Rebuilds push the latest
/// [`CanvasScene`] into the widget; the widget's [`CanvasAction`]s are routed
/// to `on_action`.
pub struct Canvas<F> {
  scene:     Arc<CanvasScene>,
  /// The palette the widget paints with.
  theme:     &'static Theme,
  /// The latest camera request; acted on when its epoch changes.
  camera:    Camera,
  /// How much of the canvas the chrome covers.
  insets:    Insets,
  /// Link mode, while it is armed.
  link:      Option<LinkMode>,
  on_action: F,
}

/// Construct a canvas view from a scene, the latest camera request, the
/// area the chrome covers, and an action handler.
///
/// The handler's return value is wrapped in [`MessageResult::Action`], which
/// is what tells the Xilem driver to re-run `app_logic` against the mutated
/// app state. (`RequestRebuild` only re-diffs the *existing* view tree, so a
/// selection change made here would never reach the widgets.)
pub fn canvas<State, Action, F>(
  scene: Arc<CanvasScene>,
  theme: &'static Theme,
  camera: Camera,
  insets: Insets,
  link: Option<LinkMode>,
  on_action: F,
) -> Canvas<impl Fn(&mut State, CanvasAction) -> MessageResult<Action>>
where
  F: Fn(&mut State, CanvasAction) -> Action + 'static,
{
  Canvas {
    scene,
    theme,
    camera,
    insets,
    link,
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
  /// The last camera epoch applied, so each request acts once.
  type ViewState = u64;

  fn build(
    &self,
    ctx: &mut ViewCtx,
    _app_state: &mut State,
  ) -> (Self::Element, Self::ViewState) {
    let pod = ctx.with_action_widget(|ctx| {
      let mut w = CanvasWidget::new(self.theme);
      w.set_scene(self.scene.clone());
      w.insets = self.insets;
      w.link = self.link.clone();
      ctx.create_pod(w)
    });
    // The canvas fits itself on first paint, whatever the request says.
    (pod, self.camera.epoch)
  }

  fn rebuild(
    &self,
    prev: &Self,
    last_epoch: &mut Self::ViewState,
    _ctx: &mut ViewCtx,
    mut element: Mut<'_, Self::Element>,
    _app_state: &mut State,
  ) {
    // The app hands back the same `Arc` while nothing the scene depends on
    // changed, so most rebuilds (panel typing, drags) skip the canvas
    // entirely instead of re-measuring every label.
    if !Arc::ptr_eq(&prev.scene, &self.scene) {
      element.widget.set_scene(self.scene.clone());
      // Labels may have changed, and with them node sizes and placement.
      element.ctx.request_layout();
      element.ctx.request_render();
    }
    if !std::ptr::eq(prev.theme, self.theme) {
      element.widget.set_theme(self.theme);
      element.ctx.request_render();
    }
    if self.insets != prev.insets {
      element.widget.insets = self.insets;
    }
    if self.link != prev.link {
      element.widget.link = self.link.clone();
      element.ctx.request_render();
    }
    if *last_epoch != self.camera.epoch {
      *last_epoch = self.camera.epoch;
      element.widget.apply(self.camera.request);
      element.ctx.request_render();
      element.ctx.request_anim_frame();
    }
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
      id:       EdgeId::from_u128(from * 100 + to),
      from:     NodeId::from_u128(from),
      to:       NodeId::from_u128(to),
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

    let down = route_edges(&[edge(1, 2)], &rects, &HashMap::new())
      .remove(0)
      .unwrap();
    assert_eq!(down.start, Point::new(75.0, 50.0));
    assert_eq!(down.end, Point::new(75.0, 100.0));
    assert_eq!(down.axis, Vec2::new(0.0, 1.0));

    let up = route_edges(&[edge(2, 1)], &rects, &HashMap::new())
      .remove(0)
      .unwrap();
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
    let routes =
      route_edges(&[edge(1, 3), edge(1, 2)], &rects, &HashMap::new());
    let (to_right, to_left) =
      (routes[0].clone().unwrap(), routes[1].clone().unwrap());
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
      via:   Vec::new(),
    };
    let (curve, tip) = edge_curve(&route);
    assert_eq!(tip, Point::new(80.0, 100.0 - TIP_GAP));
    let els = curve.elements();
    let masonry::kurbo::PathEl::CurveTo(c1, c2, base) = els[1] else {
      panic!("expected a cubic, got {els:?}");
    };
    assert_eq!(c1.x, 0.0, "leaves straight down");
    assert_eq!(c2.x, base.x, "arrives straight down");
    assert_eq!(base, Point::new(80.0, tip.y - HEAD_LEN));
  }

  /// An edge that skips rows runs straight down each channel the layout
  /// gave it, and a reversed one climbs them in the opposite order.
  #[test]
  fn skipping_edges_run_through_their_channels() {
    let top = Rect::new(0.0, 0.0, 150.0, 50.0);
    let bottom = Rect::new(0.0, 300.0, 150.0, 350.0);
    let rects = boxes(&[(1, top), (2, bottom)]);
    let channels = |id| {
      HashMap::from([(id, vec![
        Channel {
          x:      200.0,
          top:    100.0,
          bottom: 150.0,
        },
        Channel {
          x:      210.0,
          top:    200.0,
          bottom: 250.0,
        },
      ])])
    };

    let down = edge(1, 2);
    let route = route_edges(&[down.clone()], &rects, &channels(down.id))
      .remove(0)
      .unwrap();
    assert_eq!(route.via, vec![
      (Point::new(200.0, 100.0), Point::new(200.0, 150.0)),
      (Point::new(210.0, 200.0), Point::new(210.0, 250.0)),
    ]);
    // The straight runs are in the drawn path.
    let (curve, _) = edge_curve(&route);
    let lines: Vec<Point> = curve
      .elements()
      .iter()
      .filter_map(|el| match el {
        masonry::kurbo::PathEl::LineTo(p) => Some(*p),
        _ => None,
      })
      .collect();
    assert_eq!(lines, vec![
      Point::new(200.0, 150.0),
      Point::new(210.0, 250.0)
    ]);

    let up = edge(2, 1);
    let route = route_edges(&[up.clone()], &rects, &channels(up.id))
      .remove(0)
      .unwrap();
    assert_eq!(route.via, vec![
      (Point::new(210.0, 250.0), Point::new(210.0, 200.0)),
      (Point::new(200.0, 150.0), Point::new(200.0, 100.0)),
    ]);
  }

  /// Culling works in world space: the visible area follows pan and zoom,
  /// with a margin so outlines at the window edge are kept.
  #[test]
  fn visible_world_follows_pan_and_zoom() {
    let size = Size::new(800.0, 600.0);
    let m = CULL_MARGIN;
    let identity = visible_world(Affine::IDENTITY, size);
    assert_eq!(identity, Rect::new(-m, -m, 800.0 + m, 600.0 + m));

    // Zoomed to 2x and panned: the view covers half the world, offset.
    let tf = Affine::translate((100.0, 50.0)) * Affine::scale(2.0);
    let v = visible_world(tf, size);
    assert_eq!(
      v,
      Rect::new(
        (-m - 100.0) / 2.0,
        (-m - 50.0) / 2.0,
        (800.0 + m - 100.0) / 2.0,
        (600.0 + m - 50.0) / 2.0,
      )
    );
    assert!(Rect::new(0.0, 0.0, 10.0, 10.0).overlaps(v));
    assert!(!Rect::new(1000.0, 0.0, 1100.0, 50.0).overlaps(v));
  }

  /// An edge is kept while any of it (curve, channel, arrowhead) could
  /// show, including a channel far from both ends.
  #[test]
  fn route_bounds_cover_the_whole_drawn_edge() {
    let route = Route {
      start: Point::new(0.0, 0.0),
      end:   Point::new(0.0, 400.0),
      axis:  Vec2::new(0.0, 1.0),
      via:   vec![(Point::new(300.0, 100.0), Point::new(300.0, 300.0))],
    };
    let b = route.bounds();
    let (curve, tip) = edge_curve(&route);
    use masonry::kurbo::Shape as _;
    let drawn = curve.bounding_box().union_pt(tip);
    assert!(
      b.contains(drawn.origin()) && b.contains(Point::new(drawn.x1, drawn.y1))
    );
    // A view that only sees the detour still keeps the edge.
    assert!(b.overlaps(Rect::new(290.0, 150.0, 310.0, 160.0)));
  }

  /// Fitting aims at the uncovered area: the graph's centre lands on its
  /// centre, not the window's.
  #[test]
  fn fit_centres_the_graph_in_the_uncovered_area() {
    let viewport = Size::new(1000.0, 800.0);
    let insets = Insets {
      top:    40.0,
      right:  320.0,
      bottom: 0.0,
      left:   0.0,
    };
    let view = uncovered(viewport, insets);
    assert_eq!(view, Rect::new(0.0, 40.0, 680.0, 800.0));
    let bounds = Rect::new(-100.0, -50.0, 100.0, 50.0);
    let (zoom, pan) = fit(bounds, view).unwrap();
    assert_eq!(zoom, 1.0, "a small graph is not blown up");
    let centre = Affine::translate(pan) * Affine::scale(zoom) * bounds.center();
    assert_eq!(centre, view.center());

    // A big graph shrinks to fit inside the margins (down to the minimum
    // zoom, which this one does not reach).
    let big = Rect::new(0.0, 0.0, 2000.0, 1000.0);
    let (zoom, pan) = fit(big, view).unwrap();
    let shown =
      (Affine::translate(pan) * Affine::scale(zoom)).transform_rect_bbox(big);
    assert!(shown.x0 >= view.x0 + VIEW_MARGIN - 1e-9);
    assert!(shown.x1 <= view.x1 - VIEW_MARGIN + 1e-9);
  }

  /// Insets larger than the canvas leave an empty view, not an inverted one,
  /// and fitting into it does nothing.
  #[test]
  fn oversized_insets_leave_an_empty_view() {
    let insets = Insets {
      top:    0.0,
      right:  900.0,
      bottom: 0.0,
      left:   200.0,
    };
    let view = uncovered(Size::new(1000.0, 800.0), insets);
    assert_eq!(view.width(), 0.0);
    assert!(fit(Rect::new(0.0, 0.0, 10.0, 10.0), view).is_none());
  }

  /// Revealing leaves a node that is already comfortably in view alone, and
  /// centres one that is off screen or under the chrome.
  #[test]
  fn reveal_only_moves_for_hidden_nodes() {
    let view = Rect::new(0.0, 40.0, 680.0, 800.0);
    let (zoom, pan) = (1.0, Vec2::ZERO);
    let visible = Rect::new(200.0, 200.0, 350.0, 250.0);
    assert_eq!(reveal_pan(visible, zoom, pan, view), None);

    // Under the inspector card, off to the right of the view.
    let covered = Rect::new(700.0, 200.0, 850.0, 250.0);
    let target = reveal_pan(covered, zoom, pan, view).unwrap();
    let centre =
      Affine::translate(target) * Affine::scale(zoom) * covered.center();
    assert_eq!(centre, view.center());
  }
}
