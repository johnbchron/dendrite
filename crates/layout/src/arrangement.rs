//! The graph's shape before coordinates, and placing it once node sizes
//! are known (step 4 of the pipeline).

use std::collections::HashMap;

use base::{EdgeId, NodeId};

use crate::{Channel, LayoutConfig, Pos, Size, coord};

/// One place in a row: a node, or the point where a long edge passes
/// through a rank it skips.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
  /// A graph node.
  Node(NodeId),
  /// Where `edge` crosses row `rank`, strictly between its two ends.
  Bend {
    /// The edge passing through.
    edge: EdgeId,
    /// The rank it passes through.
    rank: usize,
  },
}

/// Coordinates for an [`Arrangement`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placement {
  /// Centre of every node.
  pub nodes: HashMap<NodeId, Pos>,
  /// For every edge that skips ranks, the channels it runs through, top row
  /// first. Edges between adjacent ranks have none.
  pub channels: HashMap<EdgeId, Vec<Channel>>,
}

/// The graph's shape before coordinates: which row every node (and every
/// long edge's bend) sits in, in what left-to-right order, and which tree it
/// belongs to. Everything [`Arrangement::place`] needs besides node sizes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arrangement {
  /// Slots per rank, top row first, each in left-to-right order.
  pub rows: Vec<Vec<Slot>>,
  /// The independent tree (weakly connected component) of every slot, as an
  /// opaque label: slots share a label exactly when edges connect them.
  pub tree: HashMap<Slot, usize>,
  /// Every unit-length segment between slots in adjacent ranks, upper slot
  /// first, which coordinate assignment lines slots up along.
  pub links: Vec<(Slot, Slot)>,
}

impl Arrangement {
  /// Assign every node a centre position, given its size, and every long
  /// edge the channels it runs through.
  ///
  /// Each row is as tall as its tallest node, with nodes centred on the
  /// row's midline, and rows are `y_gap` apart; rows line up across trees.
  /// Each tree takes a column of its own, trees are `tree_gap` apart (left
  /// to right in order of first appearance, reading the rows top-down), and
  /// the whole is centred on x = 0. Within a column, slots keep their order
  /// at least `x_gap` apart, lined up with their neighbours in the rows
  /// either side by [`coord::assign`]; a bend is `bend_width` wide.
  pub fn place(
    &self,
    cfg: &LayoutConfig,
    size_of: impl Fn(NodeId) -> Size,
  ) -> Placement {
    let width_of = |slot: Slot| match slot {
      Slot::Node(n) => size_of(n).w,
      Slot::Bend { .. } => cfg.bend_width,
    };
    let columns: Vec<Column> = self
      .trees()
      .into_iter()
      .map(|t| Column::new(self.slice(t), &self.links, width_of, cfg.x_gap))
      .collect();
    let total = columns.iter().map(Column::width).sum::<f64>()
      + cfg.tree_gap * columns.len().saturating_sub(1) as f64;
    let bands = self.bands(cfg, size_of);

    let mut placement = Placement::default();
    let mut left = -total / 2.0;
    for column in &columns {
      column.place_into(&mut placement, left, &bands);
      left += column.width() + cfg.tree_gap;
    }
    placement
  }

  /// Every tree's label, in order of first appearance. The barycenter sweeps
  /// put related nodes near each other, so this keeps the arrangement's
  /// reading order.
  fn trees(&self) -> Vec<usize> {
    let mut trees: Vec<usize> = Vec::new();
    for slot in self.rows.iter().flatten() {
      let t = self.tree_of(*slot);
      if !trees.contains(&t) {
        trees.push(t);
      }
    }
    trees
  }

  /// Tree `t`'s slice of each row, keeping the row's order.
  fn slice(&self, t: usize) -> Vec<Vec<Slot>> {
    self
      .rows
      .iter()
      .map(|row| {
        row
          .iter()
          .copied()
          .filter(|s| self.tree_of(*s) == t)
          .collect()
      })
      .collect()
  }

  /// Row bands, shared by every tree: `(top, height)`. Bends have no height
  /// of their own; they span whatever band the row's nodes make.
  fn bands(
    &self,
    cfg: &LayoutConfig,
    size_of: impl Fn(NodeId) -> Size,
  ) -> Vec<(f64, f64)> {
    let mut bands = Vec::with_capacity(self.rows.len());
    let mut top = 0.0;
    for row in &self.rows {
      let height = row
        .iter()
        .filter_map(|s| match s {
          Slot::Node(n) => Some(size_of(*n).h),
          Slot::Bend { .. } => None,
        })
        .fold(0.0, f64::max);
      bands.push((top, height));
      top += height + cfg.y_gap;
    }
    bands
  }

  /// The tree label of `slot`. A slot missing from [`Self::tree`] stands
  /// alone, under a label no real tree uses.
  fn tree_of(&self, slot: Slot) -> usize {
    self.tree.get(&slot).copied().unwrap_or(usize::MAX)
  }
}

/// One tree's column: its slice of every row, and where [`coord::assign`]
/// put each slot relative to the column's own left edge.
struct Column {
  /// The tree's slots, per row.
  rows: Vec<Vec<Slot>>,
  /// Each slot's centre x, at an arbitrary offset.
  xs: HashMap<Slot, f64>,
  /// The left edge of the leftmost box, at the same offset.
  lo: f64,
  /// The right edge of the rightmost box, at the same offset.
  hi: f64,
}

impl Column {
  /// Line up `rows` (one tree's slice of the arrangement) with their
  /// neighbours along `links`.
  fn new(
    rows: Vec<Vec<Slot>>,
    links: &[(Slot, Slot)],
    width_of: impl Fn(Slot) -> f64,
    gap: f64,
  ) -> Self {
    let xs = coord::assign(&rows, links, &width_of, gap);
    let (lo, hi) =
      xs.iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (s, x)| {
          let w = width_of(*s);
          (lo.min(x - w / 2.0), hi.max(x + w / 2.0))
        });
    Self { rows, xs, lo, hi }
  }

  /// How wide the column's boxes span.
  fn width(&self) -> f64 {
    (self.hi - self.lo).max(0.0)
  }

  /// Record every slot's position with the column's left edge at `left`,
  /// each row in its `bands` entry.
  fn place_into(
    &self,
    placement: &mut Placement,
    left: f64,
    bands: &[(f64, f64)],
  ) {
    for (row, &(top, height)) in self.rows.iter().zip(bands) {
      for slot in row {
        let x = left + self.xs[slot] - self.lo;
        match *slot {
          Slot::Node(n) => {
            placement.nodes.insert(
              n,
              Pos {
                x,
                y: top + height / 2.0,
              },
            );
          }
          // Rows are walked top-down, so each edge's channels come out top
          // row first.
          Slot::Bend { edge, .. } => {
            placement.channels.entry(edge).or_default().push(Channel {
              x,
              top,
              bottom: top + height,
            });
          }
        }
      }
    }
  }
}
