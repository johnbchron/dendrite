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
use layout::Pos;
use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, Widget, render_text,
  },
  kurbo::{Affine, BezPath, Line, Point, Rect, RoundedRect, Size, Vec2},
  parley::{GenericFamily, Layout as TextLayout},
  peniko::{Brush, Color, Fill},
  vello::Scene,
};
use xilem::{
  Pod, ViewCtx,
  core::{MessageContext, MessageResult, Mut, View, ViewMarker},
};

use crate::theme::{self, Theme};

// Node box size in world (graph) coordinates.
const NODE_W: f64 = 150.0;
const NODE_H: f64 = 46.0;
/// Pixels the pointer may travel between press and release and still count as
/// a click rather than a pan.
const CLICK_SLOP: f64 = 4.0;

// --- render data --------------------------------------------------------

/// One node as the canvas needs to draw and hit-test it.
#[derive(Clone, Debug)]
pub struct RenderNode {
  /// Which node this is (returned in [`CanvasAction::Select`]).
  pub id:       NodeId,
  /// Centre position in world coordinates (from `layout`).
  pub center:   Pos,
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

/// One edge as a pair of world-space endpoints plus styling flags.
#[derive(Clone, Debug)]
pub struct RenderEdge {
  /// Endpoint at the dependent / parent.
  pub from:     Pos,
  /// Endpoint at the requirement / child (the arrow points here).
  pub to:       Pos,
  /// Dependency (solid) vs. subtask (dashed).
  pub kind:     EdgeKind,
  /// Whether the cycle-cut reversed this edge (a backward cycle edge).
  pub reversed: bool,
}

/// A complete, self-contained description of what to paint.
#[derive(Clone, Debug, Default)]
pub struct CanvasScene {
  /// Nodes, painted on top of edges.
  pub nodes: Vec<RenderNode>,
  /// Edges, painted underneath.
  pub edges: Vec<RenderEdge>,
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
}

impl CanvasWidget {
  /// A fresh canvas with an identity transform and no scene.
  pub fn new(theme: &'static Theme) -> Self {
    Self {
      scene: CanvasScene::default(),
      pan: Vec2::new(60.0, 60.0),
      zoom: 1.0,
      press_origin: None,
      last_pointer: None,
      panned: false,
      needs_fit: true,
      last_size: None,
      theme,
      text_cache: HashMap::new(),
    }
  }

  /// Replace the scene and request a repaint.
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

  /// The world-space rectangle of a node centred at `center`.
  fn node_rect(center: Pos) -> Rect {
    Rect::from_center_size((center.x, center.y), (NODE_W, NODE_H))
  }

  /// Topmost node whose box contains `world`, searched front-to-back.
  fn hit_test(&self, world: Point) -> Option<NodeId> {
    self
      .scene
      .nodes
      .iter()
      .rev()
      .find(|n| Self::node_rect(n.center).contains(world))
      .map(|n| n.id)
  }

  /// Fit the whole scene into `viewport`, centred, with a margin. Never
  /// zooms in past 1:1, so a small graph stays readable rather than filling
  /// the window with a couple of giant boxes.
  fn fit_to(&mut self, viewport: masonry::kurbo::Size) {
    let mut union: Option<Rect> = None;
    for n in &self.scene.nodes {
      let r = Self::node_rect(n.center);
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
      .clamp(0.15, 4.0);

    self.zoom = zoom;
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
    self.zoom = (self.zoom * factor).clamp(0.15, 4.0);
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
          // Scroll up (positive) zooms in.
          self.zoom_about(p, (dy * 0.1).exp());
          ctx.request_render();
        }
      }
      _ => {}
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
    for edge in &self.scene.edges {
      paint_edge(scene, tf, edge, self.theme);
    }

    // Nodes on top. Split borrows: pull the caches out so `self` methods
    // that only need shaping can take &mut while we iterate a snapshot.
    let nodes = self.scene.nodes.clone();
    for node in &nodes {
      self.paint_node(ctx, scene, tf, node);
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
  /// Paint a single node: shape, fill, border and (cached) label text.
  fn paint_node(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    scene: &mut Scene,
    tf: Affine,
    node: &RenderNode,
  ) {
    let rect = Self::node_rect(node.center);
    let (fill, border) = self.theme.for_state(node.state);
    let fill = if node.dimmed { theme::dim(fill) } else { fill };

    match node.kind {
      NodeKind::Task { .. } => {
        let shape = RoundedRect::from_rect(rect, 8.0);
        scene.fill(Fill::NonZero, tf, &Brush::Solid(fill), None, &shape);
        stroke(scene, tf, &shape, border, node.selected, self.theme);
      }
      NodeKind::Condition { .. } => {
        let shape = diamond(rect);
        scene.fill(Fill::NonZero, tf, &Brush::Solid(fill), None, &shape);
        stroke(scene, tf, &shape, border, node.selected, self.theme);
      }
    }

    self.paint_label(ctx, scene, tf, node, rect);
  }

  /// Shape and paint the node label, caching the parley layout per node.
  fn paint_label(
    &mut self,
    ctx: &mut PaintCtx<'_>,
    scene: &mut Scene,
    tf: Affine,
    node: &RenderNode,
    rect: Rect,
  ) {
    let needs_build = self
      .text_cache
      .get(&node.id)
      .map(|(txt, _)| txt != &node.label)
      .unwrap_or(true);
    if needs_build {
      // Masonry's *shared* text contexts, not a private pair: a private
      // `FontContext` resolves against its own font set, so canvas labels
      // would not match the panel and would miss any font the app registers.
      let (font_cx, layout_cx) = ctx.text_contexts();
      let mut builder =
        layout_cx.ranged_builder(font_cx, &node.label, 1.0, true);
      builder.push_default(StyleProperty::FontSize(13.0));
      // Masonry's widgets get this from `theme::default_text_styles`; a
      // hand-rolled widget has to ask for it, or parley picks its own default
      // family and the canvas ends up in a different typeface to the panel.
      builder.push_default(GenericFamily::SystemUi);
      let mut layout = TextLayout::new();
      builder.build_into(&mut layout, &node.label);
      layout.break_all_lines(Some((NODE_W - 16.0) as f32));
      self
        .text_cache
        .insert(node.id, (node.label.clone(), layout));
    }
    let (_, layout) = self.text_cache.get(&node.id).unwrap();

    // Centre the first line vertically; left-pad horizontally.
    let text_h = layout.height() as f64;
    let origin = Point::new(rect.x0 + 8.0, rect.center().y - text_h / 2.0);
    let text_tf = tf * Affine::translate((origin.x, origin.y));
    render_text(
      scene,
      text_tf,
      layout,
      &[Brush::Solid(self.theme.text)],
      true,
    );
  }
}

/// Build a diamond (condition) path inscribed in `rect`.
fn diamond(rect: Rect) -> BezPath {
  let c = rect.center();
  let mut p = BezPath::new();
  p.move_to((c.x, rect.y0));
  p.line_to((rect.x1, c.y));
  p.line_to((c.x, rect.y1));
  p.line_to((rect.x0, c.y));
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

/// Distance from a node centre to its box border along the unit vector
/// `dir`: the smaller of the two slab crossings.
fn border_offset(dir: Vec2) -> f64 {
  let tx = if dir.x.abs() > 1e-9 {
    (NODE_W / 2.0) / dir.x.abs()
  } else {
    f64::INFINITY
  };
  let ty = if dir.y.abs() > 1e-9 {
    (NODE_H / 2.0) / dir.y.abs()
  } else {
    f64::INFINITY
  };
  tx.min(ty)
}

/// Paint one edge as a line plus an arrowhead at the requirement end.
fn paint_edge(scene: &mut Scene, tf: Affine, edge: &RenderEdge, theme: &Theme) {
  let a0 = Point::new(edge.from.x, edge.from.y);
  let b0 = Point::new(edge.to.x, edge.to.y);
  // Pull the endpoints back to the node borders so the line and arrowhead
  // are not hidden under the boxes. The inset has to follow the box: nodes
  // are far wider than they are tall, so a fixed radius left the arrowhead
  // buried inside the node on anything but a near-vertical edge.
  let delta = b0 - a0;
  let len = delta.hypot();
  if len <= 1e-9 {
    return;
  }
  let dir = delta / len;
  let inset = border_offset(dir) + 2.0;
  if len <= 2.0 * inset {
    // The boxes touch or overlap: the whole edge would sit under them.
    return;
  }
  let a = a0 + dir * inset;
  let b = b0 - dir * inset;
  let color = if edge.reversed {
    theme.cycle
  } else {
    theme.edge
  };
  let stroke_style = match edge.kind {
    EdgeKind::Dependency => masonry::kurbo::Stroke::new(1.5),
    _ => masonry::kurbo::Stroke::new(1.5),
  };
  scene.stroke(
    &stroke_style,
    tf,
    &Brush::Solid(color),
    None,
    &Line::new(a, b),
  );

  // Arrowhead pointing at `b`.
  let back = b - dir * 12.0;
  let perp = Vec2::new(-dir.y, dir.x) * 5.0;
  let mut head = BezPath::new();
  head.move_to((b.x, b.y));
  head.line_to((back.x + perp.x, back.y + perp.y));
  head.line_to((back.x - perp.x, back.y - perp.y));
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

  /// Edge endpoints must stop on the node box, not on a fixed radius: the
  /// boxes are far wider than tall, so a radius that suited a vertical edge
  /// left the arrowhead buried inside the node on a diagonal one.
  #[test]
  fn border_offset_lands_on_the_box_edge() {
    let half_w = NODE_W / 2.0;
    let half_h = NODE_H / 2.0;
    for d in [
      Vec2::new(1.0, 0.0),
      Vec2::new(0.0, 1.0),
      Vec2::new(1.0, 1.0),
      Vec2::new(-3.0, 1.0),
      Vec2::new(2.0, -5.0),
    ] {
      let dir = d.normalize();
      let p = dir * border_offset(dir);
      // Inside both slabs, and touching at least one of them.
      assert!(p.x.abs() <= half_w + 1e-9, "{p:?} escapes the box");
      assert!(p.y.abs() <= half_h + 1e-9, "{p:?} escapes the box");
      assert!(
        (p.x.abs() - half_w).abs() < 1e-9 || (p.y.abs() - half_h).abs() < 1e-9,
        "{p:?} stops short of the box"
      );
    }
  }
}
