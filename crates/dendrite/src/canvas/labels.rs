//! Node labels: shaped once per text and kept, and the box size each gives
//! its node; and the glyphs that mark formula conditions.

use std::{
  borrow::Cow,
  collections::{HashMap, HashSet},
};

use app::scene::Category;
use base::NodeId;
use masonry::{
  core::{BrushIndex, StyleProperty},
  parley::{
    FontContext, Layout as TextLayout, LayoutContext, LineHeight,
    style::{FontFamily, FontStack},
  },
};

use super::RenderNode;
use crate::{font, icons};

/// Width of every node box in world (graph) units, and so the width its
/// label wraps at. Height follows the label (see [`Labels::box_size`]).
const NODE_W: f64 = 175.0;
/// Space between a node's border and its label.
pub(super) const PAD_X: f64 = 10.0;
const PAD_Y: f64 = 8.0;
/// Label type size, and line height as a multiple of it.
const LABEL_SIZE: f32 = 13.0;
const LABEL_LINE: f32 = 1.3;
/// Lines of label every box leaves room for, however short its text: a
/// one-word node is not a sliver, and a two-line label does not grow its box.
const MIN_LINES: f64 = 2.0;

/// Type size of the count on a copy of a shared condition ("×3").
pub(super) const BADGE_SIZE: f32 = 10.5;

/// Size of the glyph that marks a formula condition, and the room it and
/// its gap take from the label.
const GLYPH_SIZE: f32 = 14.0;
pub(super) const GLYPH_W: f64 = 20.0;

/// What a label was shaped from: its text, and whether a glyph shares its
/// row (which narrows it).
type Shaped = (String, Option<Category>);

/// Shaped labels, keyed by box, each with what it was shaped from so a
/// rename reshapes it; the copy counts ("×3") the boxes show; and the glyph
/// for each kind of formula condition.
#[derive(Default)]
pub(super) struct Labels {
  cache: HashMap<NodeId, (Shaped, TextLayout<BrushIndex>)>,
  badges: HashMap<usize, TextLayout<BrushIndex>>,
  glyphs: HashMap<Category, TextLayout<BrushIndex>>,
}

impl Labels {
  /// Drop the labels of nodes not in `nodes`.
  pub(super) fn retain(&mut self, nodes: &[RenderNode]) {
    let live: HashSet<NodeId> = nodes.iter().map(|n| n.id).collect();
    self.cache.retain(|id, _| live.contains(id));
  }

  /// Drop every label, as when the font set changes under them.
  pub(super) fn clear(&mut self) {
    self.cache.clear();
    self.badges.clear();
    self.glyphs.clear();
  }

  /// Shape any label in `nodes` not already shaped from its current text.
  ///
  /// Takes Masonry's *shared* text contexts, not a private pair: a private
  /// `FontContext` resolves against its own font set, so canvas labels
  /// would not match the panel and would miss any font the app registers.
  pub(super) fn shape(
    &mut self,
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<BrushIndex>,
    nodes: &[RenderNode],
  ) {
    for node in nodes {
      if node.copies > 1 && !self.badges.contains_key(&node.copies) {
        let count = format!("\u{d7}{}", node.copies);
        let badge =
          Self::shape_one(font_cx, layout_cx, &count, BADGE_SIZE, font::STACK);
        self.badges.insert(node.copies, badge);
      }
      if let Some(category) = node.glyph
        && !self.glyphs.contains_key(&category)
      {
        let glyph = String::from(icons::Icon::for_category(category).glyph());
        let stack =
          FontStack::Single(FontFamily::Named(Cow::Borrowed(icons::FAMILY)));
        let layout =
          Self::shape_one(font_cx, layout_cx, &glyph, GLYPH_SIZE, stack);
        self.glyphs.insert(category, layout);
      }
      let shaped = (node.label.clone(), node.glyph);
      let fresh = self
        .cache
        .get(&node.id)
        .is_some_and(|(from, _)| *from == shaped);
      if fresh {
        continue;
      }
      let mut text = Self::shape_one(
        font_cx,
        layout_cx,
        &node.label,
        LABEL_SIZE,
        font::STACK,
      );
      let glyph_w = if node.glyph.is_some() { GLYPH_W } else { 0.0 };
      text.break_all_lines(Some((NODE_W - 2.0 * PAD_X - glyph_w) as f32));
      self.cache.insert(node.id, (shaped, text));
    }
  }

  /// Shape `text` at `size` in `stack`, unbroken.
  fn shape_one(
    font_cx: &mut FontContext,
    layout_cx: &mut LayoutContext<BrushIndex>,
    text: &str,
    size: f32,
    stack: FontStack<'static>,
  ) -> TextLayout<BrushIndex> {
    let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::LineHeight(
      LineHeight::FontSizeRelative(LABEL_LINE),
    ));
    // A hand-rolled widget has to ask for the app face itself, or parley
    // picks its own default and the canvas ends up in a different typeface
    // to the panel.
    builder.push_default(StyleProperty::FontStack(stack));
    let mut layout = TextLayout::new();
    builder.build_into(&mut layout, text);
    layout.break_all_lines(None);
    layout
  }

  /// The shaped label of `node`, if it has one.
  pub(super) fn get(&self, node: NodeId) -> Option<&TextLayout<BrushIndex>> {
    self.cache.get(&node).map(|(_, text)| text)
  }

  /// The shaped glyph for a formula condition about `category`.
  pub(super) fn glyph(
    &self,
    category: Category,
  ) -> Option<&TextLayout<BrushIndex>> {
    self.glyphs.get(&category)
  }

  /// The shaped count for a node drawn `copies` times, if it is shaped.
  pub(super) fn badge(&self, copies: usize) -> Option<&TextLayout<BrushIndex>> {
    self.badges.get(&copies)
  }

  /// The box size of every labelled node.
  pub(super) fn sizes(&self) -> HashMap<NodeId, layout::Size> {
    self
      .cache
      .iter()
      .map(|(id, (_, text))| (*id, Self::box_size(text)))
      .collect()
  }

  /// A node's box size for its shaped label: the fixed width, and tall
  /// enough for every line of the label but never fewer than [`MIN_LINES`].
  fn box_size(text: &TextLayout<BrushIndex>) -> layout::Size {
    let min_text = MIN_LINES * f64::from(LABEL_SIZE * LABEL_LINE);
    layout::Size {
      w: NODE_W,
      h: (text.height() as f64).max(min_text) + 2.0 * PAD_Y,
    }
  }
}
