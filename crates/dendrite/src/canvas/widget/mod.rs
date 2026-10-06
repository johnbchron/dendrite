//! The Masonry canvas widget. Owns the view transform (pan/zoom) and the
//! interaction state; the scene is pushed in from the view.

mod motion;
mod pointer;
mod tween;

use std::{
  collections::{HashMap, HashSet},
  sync::Arc,
};

use app::camera::{ZOOM_RESET, zoom_percent};
use base::{EdgeId, NodeId};
use layout::LayoutConfig;
use masonry::{
  accesskit::{Node as AccessNode, Role},
  core::{
    AccessCtx, BoxConstraints, ChildrenIds, CursorIcon, EventCtx, LayoutCtx,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, QueryCtx,
    RegisterCtx, UpdateCtx, Widget,
  },
  kurbo::{Point, Rect, Size, Vec2},
  vello::Scene,
};

use self::{
  pointer::Press,
  tween::{Drawn, Placed, Tween},
};
use super::{
  CanvasAction, CanvasScene, Insets, LinkMode, RenderEdge, RenderNode,
  frame::Frame, labels::Labels, paint::Painter, route::Route,
};
use crate::theme::Theme;

/// How large a box is drawn as it starts to fade in, or finishes fading
/// out, relative to its full size.
const FADE_SCALE: f64 = 0.92;

/// A box under the pointer: which copy, and the node it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Hit {
  /// The box.
  copy: NodeId,
  /// The graph node it draws.
  node: NodeId,
}

/// The Masonry canvas widget.
pub struct CanvasWidget {
  scene: Arc<CanvasScene>,
  /// Whether labels, sizes, placement and routes need recomputing in the
  /// next layout pass: set by a new scene or a font change, and cleared
  /// once done, so resizes and repaints do not redo them.
  dirty: bool,
  /// The world→screen transform.
  frame: Frame,
  /// The scale the view is animating towards; wheel input moves this, and
  /// each animation frame eases the frame's zoom after it.
  zoom_target: f64,
  /// Screen point the animated zoom is anchored on (the last wheel position).
  zoom_anchor: Point,
  /// A requested fit not yet started: it starts on the next animation
  /// frame that has placed boxes to fit.
  fit_pending: bool,
  /// The frame an eased fit or reveal is heading for, if one is in flight.
  glide: Option<Frame>,
  /// A box to reveal once it has been placed (the request can arrive in the
  /// same rebuild as the scene that adds it).
  reveal: Option<NodeId>,
  /// How much of the canvas the chrome covers.
  insets: Insets,
  /// Link mode, while it is armed.
  link: Option<LinkMode>,
  /// The box under the pointer, tracked while no button is held.
  hover: Option<Hit>,
  /// The zoom percentage last reported to the app, so it hears of changes
  /// only.
  reported_zoom: u32,
  /// The press in progress, if any, for click-vs-pan discrimination.
  press: Option<Press>,
  /// When set, the next layout fits the whole graph into the viewport. Set
  /// on construction (so the app opens centred) and on a resize (PLAN §5,
  /// §6.3).
  needs_fit: bool,
  /// Viewport size from the previous layout pass, so a resize can request
  /// a refit (the existing fit is centred on the old viewport).
  last_size: Option<Size>,
  /// The palette every colour painted here comes from, pushed in from the
  /// view on rebuild.
  theme: &'static Theme,
  /// Shaped node labels.
  labels: Labels,
  /// Every box and edge in world coordinates, placed in the layout pass
  /// from the scene's arrangement and the measured labels: where they are,
  /// or are heading while a relayout eases in.
  placed: Placed,
  /// The relayout easing in, while one is.
  tween: Option<Tween>,
  /// Every box and edge as drawn and hit-tested this frame: `placed`, or on
  /// its way there, with what left the scene fading out.
  shown: Drawn,
  /// Nodes that left the scene, kept to draw while they fade out.
  gone: HashMap<NodeId, RenderNode>,
  /// Edges that left the scene, likewise.
  gone_edges: HashMap<EdgeId, RenderEdge>,
  /// The edges drawn: the scene's, then any fading out.
  edges: Vec<RenderEdge>,
  /// Where each drawn edge attaches, index-aligned with `edges`; routed
  /// alongside `shown`.
  routes: Vec<Option<Route>>,
}

impl CanvasWidget {
  /// A fresh canvas at the reset zoom, and no scene.
  pub fn new(theme: &'static Theme) -> Self {
    Self {
      scene: Arc::default(),
      dirty: true,
      frame: Frame {
        zoom: ZOOM_RESET,
        pan: Vec2::new(60.0, 60.0),
      },
      zoom_target: ZOOM_RESET,
      zoom_anchor: Point::ORIGIN,
      fit_pending: false,
      glide: None,
      reveal: None,
      insets: Insets::default(),
      reported_zoom: zoom_percent(ZOOM_RESET),
      link: None,
      hover: None,
      press: None,
      needs_fit: true,
      last_size: None,
      theme,
      labels: Labels::default(),
      placed: Placed::default(),
      tween: None,
      shown: Drawn::default(),
      gone: HashMap::new(),
      gone_edges: HashMap::new(),
      edges: Vec::new(),
      routes: Vec::new(),
    }
  }

  /// Replace the scene. The caller must request a layout, which re-measures
  /// and re-places the nodes.
  pub(super) fn set_scene(&mut self, scene: Arc<CanvasScene>) {
    // Keep what leaves the scene (and its label) until it has faded out;
    // what comes back is the scene's again.
    let nodes: HashSet<NodeId> = scene.nodes.iter().map(|n| n.id).collect();
    let edges: HashSet<EdgeId> = scene.edges.iter().map(|e| e.id).collect();
    for node in &self.scene.nodes {
      self.gone.insert(node.id, node.clone());
    }
    for edge in &self.scene.edges {
      self.gone_edges.insert(edge.id, edge.clone());
    }
    self.gone.retain(|id, _| !nodes.contains(id));
    self.gone_edges.retain(|id, _| !edges.contains(id));
    self.scene = scene;
    self.dirty = true;
  }

  /// Swap the palette. Colours are read at paint time, so storing it is the
  /// whole job.
  pub(super) fn set_theme(&mut self, theme: &'static Theme) {
    self.theme = theme;
  }

  /// Record how much of the canvas the chrome covers.
  pub(super) fn set_insets(&mut self, insets: Insets) {
    self.insets = insets;
  }

  /// Arm, re-aim or disarm link mode.
  pub(super) fn set_link(&mut self, link: Option<LinkMode>) {
    self.link = link;
  }

  /// Topmost box containing `world`, searched front-to-back.
  fn hit_test(&self, world: Point) -> Option<Hit> {
    self
      .scene
      .nodes
      .iter()
      .rev()
      .find(|n| {
        let rect = self.shown.placed.rects.get(&n.id);
        rect.is_some_and(|r| r.contains(world))
      })
      .map(|n| Hit {
        copy: n.id,
        node: n.node,
      })
  }

  /// Shape any label not already shaped, then size and place every node,
  /// easing from what is drawn now to the new placement.
  fn measure(&mut self, ctx: &mut LayoutCtx<'_>) {
    let (font_cx, layout_cx) = ctx.text_contexts();
    self.labels.shape(font_cx, layout_cx, &self.scene.nodes);

    let sizes = self.labels.sizes();
    let cfg = LayoutConfig::default();
    let placed = self
      .scene
      .arrangement
      .place(&cfg, |id| sizes.get(&id).copied().unwrap_or(cfg.node_size));
    let centres = &placed.nodes;
    let rects = self
      .scene
      .nodes
      .iter()
      .filter_map(|n| {
        let c = centres.get(&n.id)?;
        let size = sizes.get(&n.id)?;
        Some((n.id, Rect::from_center_size((c.x, c.y), (size.w, size.h))))
      })
      .collect();
    let to = Placed {
      rects,
      edges: self.scene.edges.iter().map(|e| e.id).collect(),
      channels: placed.channels,
    };
    self.tween = Tween::between(self.drawn(), &to);
    self.placed = to;
    self.show();
  }

  /// What to draw this frame: the placement, or the way there while a
  /// relayout eases in.
  fn drawn(&self) -> Drawn {
    match &self.tween {
      Some(tween) => tween.at(&self.placed),
      None => Drawn::settled(self.placed.clone()),
    }
  }

  /// Draw and hit-test [`Self::drawn`], routing every edge between it.
  /// Once nothing is easing in, what left the scene is let go.
  fn show(&mut self) {
    self.shown = self.drawn();
    if self.tween.is_none() {
      self.gone.clear();
      self.gone_edges.clear();
      self.labels.retain(&self.scene.nodes);
    }
    let drawn = &self.shown.placed;
    self.edges = self
      .scene
      .edges
      .iter()
      .chain(
        self
          .gone_edges
          .values()
          .filter(|e| drawn.edges.contains(&e.id)),
      )
      .cloned()
      .collect();
    self.routes = Route::for_edges(&self.edges, &drawn.rects, &drawn.channels);
  }

  /// The canvas fills whatever its parent offers, falling back to a sane
  /// size when unconstrained.
  fn size_for(bc: &BoxConstraints) -> Size {
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
}

impl Widget for CanvasWidget {
  type Action = CanvasAction;

  fn on_pointer_event(
    &mut self,
    ctx: &mut EventCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    event: &PointerEvent,
  ) {
    self.pointer(ctx, event);
  }

  fn on_anim_frame(
    &mut self,
    ctx: &mut UpdateCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    interval: u64,
  ) {
    self.animate(ctx, interval);
  }

  fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

  fn layout(
    &mut self,
    ctx: &mut LayoutCtx<'_>,
    _props: &mut PropertiesMut<'_>,
    bc: &BoxConstraints,
  ) -> Size {
    // Cached layouts were shaped against the old font set (masonry asks that
    // this be checked in the layout pass, not at paint time).
    if ctx.fonts_changed() {
      self.labels.clear();
      self.dirty = true;
    }
    if self.dirty {
      self.measure(ctx);
      self.dirty = false;
    }
    let size = Self::size_for(bc);
    // A resize leaves the graph off-centre, so refit.
    if self.last_size.is_some_and(|prev| prev != size) {
      self.needs_fit = true;
    }
    self.last_size = Some(size);
    // Fit once the viewport size and the node boxes are both known: at
    // startup, after a resize, and on request.
    if self.needs_fit && !self.placed.rects.is_empty() {
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
    let view = self.frame.visible_world(size);
    let mut painter = Painter::new(scene, self.frame.affine(), self.theme);
    painter.background(size);

    // Edges under nodes. Only what can be seen is encoded: at scale most of
    // a big graph is off screen, and each label is a text run to render.
    // An edge fades with its ends, as well as on its own.
    let shown = &self.shown;
    for (edge, route) in self.edges.iter().zip(&self.routes) {
      if let Some(route) = route
        && route.bounds().overlaps(view)
      {
        let opacity = shown
          .of_edge(edge.id)
          .min(shown.of_box(edge.from))
          .min(shown.of_box(edge.to));
        painter.faded(opacity, 1.0, route.bounds(), |p| p.edge(edge, route));
      }
    }

    // In link mode, the edge a click on the hovered node would add.
    if let Some(link) = &self.link
      && let Some(target) = self.hover
      && !link.taken.contains(&target.node)
      && let (Some(&from), Some(&to)) = (
        shown.placed.rects.get(&link.source),
        shown.placed.rects.get(&target.copy),
      )
    {
      let color = if link.closes_cycle.contains(&target.node) {
        self.theme.cycle
      } else {
        self.theme.accent
      };
      painter.preview(from, to, color);
    }

    // Nodes on top, any fading out underneath the rest. A fading box also
    // grows in, or shrinks away, a little.
    for node in self.gone.values().chain(&self.scene.nodes) {
      if let Some(&rect) = shown.placed.rects.get(&node.id)
        && rect.overlaps(view)
      {
        let opacity = shown.of_box(node.id);
        let scale = FADE_SCALE + (1.0 - FADE_SCALE) * opacity;
        painter.faded(opacity, scale, rect, |p| {
          p.node(
            node,
            rect,
            self.link.as_ref(),
            self.labels.get(node.id),
            self.labels.badge(node.copies),
            node.glyph.and_then(|g| self.labels.glyph(g)),
          );
        });
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
    ChildrenIds::new()
  }
}
