//! Drawing the graph into a Vello scene.

use std::collections::HashMap;

use app::scene::Membership;
use base::{EdgeId, NodeId, NodeKind};
use masonry::{
  core::{BrushIndex, render_text},
  kurbo::{
    Affine, BezPath, Cap, Circle, Line, Point, Rect, RoundedRect, Shape, Size,
    Stroke,
  },
  parley::Layout as TextLayout,
  peniko::{Brush, Color, Fill},
  vello::Scene,
};

use super::{
  LinkMode, RenderEdge, RenderNode,
  labels::{GLYPH_W, PAD_X},
  route::Route,
};
use crate::theme::Theme;

/// How far a condition's corners are cut back, in world units.
const CHAMFER: f64 = 10.0;
/// Radius of the ring where an edge meets a copy of a shared condition.
pub(super) const COPY_RING: f64 = 5.0;
/// Space either side of a copy count inside its pill.
const BADGE_PAD: f64 = 5.0;
/// How far the quest bar sits in from a box's left edge, and stops short
/// of its top and bottom (clear of a condition's cut corners).
const QUEST_INSET: (f64, f64) = (4.5, 9.0);

/// One paint pass: the scene being drawn into, the world→screen transform
/// and the palette.
pub(super) struct Painter<'a> {
  scene: &'a mut Scene,
  tf:    Affine,
  theme: &'static Theme,
}

impl<'a> Painter<'a> {
  /// Paint into `scene`, taking world coordinates through `tf`.
  pub(super) fn new(
    scene: &'a mut Scene,
    tf: Affine,
    theme: &'static Theme,
  ) -> Self {
    Self { scene, tf, theme }
  }

  /// Fill a `size`-sized viewport with the canvas ground.
  pub(super) fn background(&mut self, size: Size) {
    self.scene.fill(
      Fill::NonZero,
      Affine::IDENTITY,
      &Brush::Solid(self.theme.bg),
      None,
      &Rect::from_origin_size((0.0, 0.0), (size.width, size.height)),
    );
  }

  /// Paint one edge as a curve plus an arrowhead at the dependent end, and
  /// a ring at the requirement end when that is one of several copies.
  pub(super) fn edge(&mut self, edge: &RenderEdge, route: &Route) {
    let color = if edge.reversed {
      self.theme.cycle
    } else {
      self.theme.edge
    };
    let (curve, tip) = route.curve();
    self.scene.stroke(
      &Stroke::new(1.5),
      self.tf,
      &Brush::Solid(color),
      None,
      &curve,
    );
    self.scene.fill(
      Fill::NonZero,
      self.tf,
      &Brush::Solid(color),
      None,
      &route.head(tip),
    );
    if edge.to_copy {
      // Hollow, sitting on the line just clear of the box, so it reads as
      // a mark on the edge rather than a part of the node.
      let ring = Circle::new(route.end - route.axis * COPY_RING, COPY_RING);
      self.scene.fill(
        Fill::NonZero,
        self.tf,
        &Brush::Solid(self.theme.bg),
        None,
        &ring,
      );
      self.scene.stroke(
        &Stroke::new(1.5),
        self.tf,
        &Brush::Solid(color),
        None,
        &ring,
      );
    }
  }

  /// Paint the edge link mode would add from `from` to `to`: dashed, in
  /// `color`, routed like a real edge (without channels, since it does not
  /// exist yet).
  pub(super) fn preview(&mut self, from: Rect, to: Rect, color: Color) {
    let (a, b) = (NodeId::from_u128(0), NodeId::from_u128(1));
    let edge = RenderEdge {
      id:       EdgeId::from_u128(0),
      from:     a,
      to:       b,
      reversed: false,
      to_copy:  false,
    };
    let rects = HashMap::from([(a, from), (b, to)]);
    let Some(route) =
      Route::for_edges(&[edge], &rects, &HashMap::new()).remove(0)
    else {
      return;
    };
    let (curve, tip) = route.curve();
    self.scene.stroke(
      &Stroke::new(2.0).with_dashes(0.0, [6.0, 4.0]),
      self.tf,
      &Brush::Solid(color),
      None,
      &curve,
    );
    self.scene.fill(
      Fill::NonZero,
      self.tf,
      &Brush::Solid(color),
      None,
      &route.head(tip),
    );
  }

  /// Paint a single node in `rect`: shape, fill, border and `label`, the
  /// `glyph` of a formula condition before it, and for a copy of a shared
  /// condition, its `badge` ("×3"). In `link` mode, nodes a click cannot
  /// add fade back and nodes that would close a cycle get a warning ring.
  pub(super) fn node(
    &mut self,
    node: &RenderNode,
    rect: Rect,
    link: Option<&LinkMode>,
    label: Option<&TextLayout<BrushIndex>>,
    badge: Option<&TextLayout<BrushIndex>>,
    glyph: Option<&TextLayout<BrushIndex>>,
  ) {
    let (fill, border) = self.theme.for_state(node.state);
    let taken = link.is_some_and(|l| l.taken.contains(&node.node));
    let fill = if node.dimmed || taken {
      Theme::dim(fill)
    } else {
      fill
    };
    let border = if taken { Theme::dim(border) } else { border };

    match node.kind {
      NodeKind::Task { .. } => {
        let shape = RoundedRect::from_rect(rect, 8.0);
        self.fill_and_outline(&shape, fill, border, node.selected);
      }
      NodeKind::Condition { .. } => {
        let shape = Self::chamfered(rect);
        self.fill_and_outline(&shape, fill, border, node.selected);
      }
    }

    self.quest_bar(rect, node.quest, node.dimmed || taken);

    if link.is_some_and(|l| l.closes_cycle.contains(&node.node)) {
      let ring = RoundedRect::from_rect(rect.inflate(4.0, 4.0), 11.0);
      self.scene.stroke(
        &Stroke::new(2.0).with_dashes(0.0, [6.0, 4.0]),
        self.tf,
        &Brush::Solid(self.theme.cycle),
        None,
        &ring,
      );
    }

    // The glyph sits at the start of the label's row, centred on the box.
    let mut text_x = rect.x0 + PAD_X;
    if node.glyph.is_some() {
      if let Some(glyph) = glyph {
        let h = glyph.height() as f64;
        let origin = Point::new(text_x, rect.center().y - h / 2.0);
        render_text(
          self.scene,
          self.tf * Affine::translate(origin.to_vec2()),
          glyph,
          &[Brush::Solid(self.theme.muted)],
          true,
        );
      }
      text_x += GLYPH_W;
    }

    // Labels are shaped in the layout pass; one is only missing if this
    // paint raced a scene change, and the next frame will have it.
    if let Some(text) = label {
      // Centred vertically, so a box held open at its minimum height does
      // not leave a one-line label stuck to its top.
      let text_h = text.height() as f64;
      let origin = Point::new(text_x, rect.center().y - text_h / 2.0);
      render_text(
        self.scene,
        self.tf * Affine::translate(origin.to_vec2()),
        text,
        &[Brush::Solid(self.theme.text)],
        true,
      );
    }

    if node.copies > 1
      && let Some(text) = badge
    {
      self.badge(rect, text, border);
    }
  }

  /// A bar down the inside of `rect`'s left edge for a node in a quest:
  /// solid in the accent when a quest claims it, thin and muted when it is
  /// only required by something claimed. Faded with the rest of a `dimmed`
  /// box.
  fn quest_bar(&mut self, rect: Rect, quest: Membership, dimmed: bool) {
    let (color, width) = match quest {
      Membership::None => return,
      Membership::Indirect => (self.theme.muted, 1.5),
      Membership::Direct => (self.theme.accent, 3.0),
    };
    let color = if dimmed { Theme::dim(color) } else { color };
    let (dx, dy) = QUEST_INSET;
    let x = rect.x0 + dx;
    self.scene.stroke(
      &Stroke::new(width).with_caps(Cap::Round),
      self.tf,
      &Brush::Solid(color),
      None,
      &Line::new((x, rect.y0 + dy), (x, rect.y1 - dy)),
    );
  }

  /// A copy count in a pill straddling the top edge of `rect`, towards its
  /// right-hand end, outlined in the node's `border` colour.
  fn badge(
    &mut self,
    rect: Rect,
    text: &TextLayout<BrushIndex>,
    border: Color,
  ) {
    let (w, h) = (text.width() as f64, text.height() as f64);
    let pill = Rect::from_center_size(
      (rect.x1 - CHAMFER - BADGE_PAD - w / 2.0 - 4.0, rect.y0),
      (w + 2.0 * BADGE_PAD, h + 2.0),
    );
    let shape = RoundedRect::from_rect(pill, pill.height() / 2.0);
    self.scene.fill(
      Fill::NonZero,
      self.tf,
      &Brush::Solid(self.theme.bg),
      None,
      &shape,
    );
    self.scene.stroke(
      &Stroke::new(1.0),
      self.tf,
      &Brush::Solid(border),
      None,
      &shape,
    );
    let origin =
      Point::new(pill.center().x - w / 2.0, pill.center().y - h / 2.0);
    render_text(
      self.scene,
      self.tf * Affine::translate(origin.to_vec2()),
      text,
      &[Brush::Solid(self.theme.text)],
      true,
    );
  }

  /// Fill `shape`, then stroke its border, thicker and accented when
  /// selected.
  fn fill_and_outline(
    &mut self,
    shape: &impl Shape,
    fill: Color,
    border: Color,
    selected: bool,
  ) {
    self
      .scene
      .fill(Fill::NonZero, self.tf, &Brush::Solid(fill), None, shape);
    let (color, width) = if selected {
      (self.theme.accent, 3.0)
    } else {
      (border, 1.5)
    };
    self.scene.stroke(
      &Stroke::new(width),
      self.tf,
      &Brush::Solid(color),
      None,
      shape,
    );
  }

  /// A condition's outline: `rect` with its corners cut off at 45°.
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
}
