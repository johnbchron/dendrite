//! The Masonry canvas widget. Owns the view transform (pan/zoom) and the
//! interaction state; the scene is pushed in from the view.

mod motion;
mod pointer;

use std::{collections::HashMap, sync::Arc};

use base::NodeId;
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

use self::pointer::Press;
use super::{
  CanvasAction, CanvasScene, Insets, LinkMode, frame::Frame, labels::Labels,
  paint::Painter, route::Route,
};
use crate::theme::Theme;

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
  scene:         Arc<CanvasScene>,
  /// Whether labels, sizes, placement and routes need recomputing in the
  /// next layout pass: set by a new scene or a font change, and cleared
  /// once done, so resizes and repaints do not redo them.
  dirty:         bool,
  /// The world→screen transform.
  frame:         Frame,
  /// The scale the view is animating towards; wheel input moves this, and
  /// each animation frame eases the frame's zoom after it.
  zoom_target:   f64,
  /// Screen point the animated zoom is anchored on (the last wheel position).
  zoom_anchor:   Point,
  /// A requested fit not yet started: it starts on the next animation
  /// frame that has placed boxes to fit.
  fit_pending:   bool,
  /// The frame an eased fit or reveal is heading for, if one is in flight.
  glide:         Option<Frame>,
  /// A box to reveal once it has been placed (the request can arrive in the
  /// same rebuild as the scene that adds it).
  reveal:        Option<NodeId>,
  /// How much of the canvas the chrome covers.
  insets:        Insets,
  /// Link mode, while it is armed.
  link:          Option<LinkMode>,
  /// The box under the pointer, tracked while no button is held.
  hover:         Option<Hit>,
  /// The zoom percentage last reported to the app, so it hears of changes
  /// only.
  reported_zoom: u32,
  /// The press in progress, if any, for click-vs-pan discrimination.
  press:         Option<Press>,
  /// When set, the next layout fits the whole graph into the viewport. Set
  /// on construction (so the app opens centred) and on a resize (PLAN §5,
  /// §6.3).
  needs_fit:     bool,
  /// Viewport size from the previous layout pass, so a resize can request
  /// a refit (the existing fit is centred on the old viewport).
  last_size:     Option<Size>,
  /// The palette every colour painted here comes from, pushed in from the
  /// view on rebuild.
  theme:         &'static Theme,
  /// Shaped node labels.
  labels:        Labels,
  /// Every box in world coordinates, placed in the layout pass from
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
      frame: Frame {
        zoom: 1.0,
        pan:  Vec2::new(60.0, 60.0),
      },
      zoom_target: 1.0,
      zoom_anchor: Point::ORIGIN,
      fit_pending: false,
      glide: None,
      reveal: None,
      insets: Insets::default(),
      reported_zoom: 100,
      link: None,
      hover: None,
      press: None,
      needs_fit: true,
      last_size: None,
      theme,
      labels: Labels::default(),
      rects: HashMap::new(),
      routes: Vec::new(),
    }
  }

  /// Replace the scene. The caller must request a layout, which re-measures
  /// and re-places the nodes.
  pub(super) fn set_scene(&mut self, scene: Arc<CanvasScene>) {
    // Drop cached text for nodes that vanished.
    self.labels.retain(&scene.nodes);
    self.scene = scene;
    self.dirty = true;
  }

  /// Swap the palette. Colours are read at paint time, so storing it is the
  /// whole job.
  pub(super) fn set_theme(&mut self, theme: &'static Theme) {
    self.theme = theme;
  }

  /// Record how much of the canvas the chrome covers.
  pub(super) fn set_insets(&mut self, insets: Insets) { self.insets = insets; }

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
      .find(|n| self.rects.get(&n.id).is_some_and(|r| r.contains(world)))
      .map(|n| Hit {
        copy: n.id,
        node: n.node,
      })
  }

  /// Shape any label not already shaped, then size and place every node
  /// and route every edge.
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
    self.routes =
      Route::for_edges(&self.scene.edges, &self.rects, &placed.channels);
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
    let view = self.frame.visible_world(size);
    let mut painter = Painter::new(scene, self.frame.affine(), self.theme);
    painter.background(size);

    // Edges under nodes. Only what can be seen is encoded: at scale most of
    // a big graph is off screen, and each label is a text run to render.
    for (edge, route) in self.scene.edges.iter().zip(&self.routes) {
      if let Some(route) = route
        && route.bounds().overlaps(view)
      {
        painter.edge(edge, route);
      }
    }

    // In link mode, the edge a click on the hovered node would add.
    if let Some(link) = &self.link
      && let Some(target) = self.hover
      && !link.taken.contains(&target.node)
      && let (Some(&from), Some(&to)) =
        (self.rects.get(&link.source), self.rects.get(&target.copy))
    {
      let color = if link.closes_cycle.contains(&target.node) {
        self.theme.cycle
      } else {
        self.theme.accent
      };
      painter.preview(from, to, color);
    }

    // Nodes on top.
    for node in &self.scene.nodes {
      if let Some(&rect) = self.rects.get(&node.id)
        && rect.overlaps(view)
      {
        painter.node(
          node,
          rect,
          self.link.as_ref(),
          self.labels.get(node.id),
          self.labels.badge(node.copies),
        );
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
