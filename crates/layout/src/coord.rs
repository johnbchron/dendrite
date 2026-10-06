//! Horizontal coordinate assignment (PLAN §4 step 4), after Brandes and
//! Köpf, "Fast and Simple Horizontal Coordinate Assignment" (2001).
//!
//! Every slot is lined up with the median of its neighbours in the row
//! above, so a node sits over what it depends on and under what depends on
//! it, and a chain of nodes (or a long edge's bends) runs straight down.
//! Aligned slots form *blocks* that share an x; blocks are then pushed as
//! close together as the rows' order and gaps allow.
//!
//! One pass of that leans towards one side, so it is done four ways — each
//! of top-down and bottom-up, packed left and packed right — and every slot
//! takes the average of its two middle positions, which cancels the lean.
//!
//! Unlike the paper, slots have widths of their own: blocks are kept apart
//! by half of each neighbour's width plus the gap, centre to centre. The
//! compaction is the two-pass longest path over the block graph that dagre
//! uses, rather than the paper's classes and sinks.

use std::collections::{HashMap, HashSet};

use crate::Slot;

/// The centre x of every slot in `rows`, each row's slots left to right in
/// the order given and at least `gap` apart, box edge to box edge.
///
/// `links` are unit-length segments, upper slot first; only those joining
/// slots in adjacent rows count. The result's offset is arbitrary: callers
/// place the whole by its extent.
pub fn assign(
  rows: &[Vec<Slot>],
  links: &[(Slot, Slot)],
  width: impl Fn(Slot) -> f64,
  gap: f64,
) -> HashMap<Slot, f64> {
  let grid = Grid::new(rows, links, width);
  let conflicts = grid.conflicts();

  let mut layouts = Vec::with_capacity(4);
  for down in [true, false] {
    for left in [true, false] {
      let dir = Direction { down, left };
      let root = grid.align(&conflicts, dir);
      layouts.push((dir, grid.compact(&root, dir, gap)));
    }
  }

  // Line the four up against the narrowest: those packed left by their
  // left edges, those packed right by their right edges.
  let extents: Vec<(f64, f64)> =
    layouts.iter().map(|(_, xs)| grid.extent(xs)).collect();
  let narrowest = extents
    .iter()
    .copied()
    .min_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
    .unwrap_or((0.0, 0.0));
  for ((dir, xs), (lo, hi)) in layouts.iter_mut().zip(extents) {
    let shift = if dir.left {
      narrowest.0 - lo
    } else {
      narrowest.1 - hi
    };
    xs.iter_mut().for_each(|x| *x += shift);
  }

  (0..grid.slots.len())
    .map(|v| {
      let mut xs = [0.0; 4];
      for (k, (_, layout)) in layouts.iter().enumerate() {
        xs[k] = layout[v];
      }
      xs.sort_by(f64::total_cmp);
      (grid.slots[v], (xs[1] + xs[2]) / 2.0)
    })
    .collect()
}

/// Which way one of the four passes runs.
#[derive(Clone, Copy, Debug)]
struct Direction {
  /// Align with the row above (top-down), or the row below (bottom-up).
  down: bool,
  /// Pack towards the left, or the right.
  left: bool,
}

/// The rows with every slot numbered, and its neighbours in the rows either
/// side, by number.
struct Grid {
  slots: Vec<Slot>,
  rows: Vec<Vec<usize>>,
  /// Each slot's row.
  layer: Vec<usize>,
  /// Each slot's index within its row.
  pos: Vec<usize>,
  width: Vec<f64>,
  /// Neighbours in the row above, left to right.
  up: Vec<Vec<usize>>,
  /// Neighbours in the row below, left to right.
  down: Vec<Vec<usize>>,
}

impl Grid {
  fn new(
    rows: &[Vec<Slot>],
    links: &[(Slot, Slot)],
    width: impl Fn(Slot) -> f64,
  ) -> Grid {
    let slots: Vec<Slot> = rows.iter().flatten().copied().collect();
    let index: HashMap<Slot, usize> =
      slots.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let mut layer = vec![0; slots.len()];
    let mut pos = vec![0; slots.len()];
    let mut numbered = Vec::with_capacity(rows.len());
    for (r, row) in rows.iter().enumerate() {
      numbered.push(
        row
          .iter()
          .enumerate()
          .map(|(i, s)| {
            let v = index[s];
            layer[v] = r;
            pos[v] = i;
            v
          })
          .collect(),
      );
    }
    let mut up = vec![Vec::new(); slots.len()];
    let mut down = vec![Vec::new(); slots.len()];
    for (a, b) in links {
      let (Some(&a), Some(&b)) = (index.get(a), index.get(b)) else {
        continue;
      };
      if layer[b] == layer[a] + 1 {
        up[b].push(a);
        down[a].push(b);
      }
    }
    for list in up.iter_mut().chain(down.iter_mut()) {
      list.sort_by_key(|&v| pos[v]);
      list.dedup();
    }
    Grid {
      width: slots.iter().map(|s| width(*s)).collect(),
      slots,
      rows: numbered,
      layer,
      pos,
      up,
      down,
    }
  }

  /// Whether the segment from `u` (above) to `v` (below) is part of a long
  /// edge's run between two bends.
  fn inner(&self, u: usize, v: usize) -> bool {
    matches!(self.slots[u], Slot::Bend { .. })
      && matches!(self.slots[v], Slot::Bend { .. })
  }

  /// Segments (upper, lower) that cross a long edge's inner segment. They
  /// give way when aligning, so long edges stay straight.
  fn conflicts(&self) -> HashSet<(usize, usize)> {
    let mut marked = HashSet::new();
    for pair in self.rows.windows(2) {
      let (upper, lower) = (&pair[0], &pair[1]);
      let mut k0 = 0;
      let mut scanned = 0;
      for (l1, &v) in lower.iter().enumerate() {
        let inner = self.up[v].iter().copied().find(|&u| self.inner(u, v));
        if l1 + 1 != lower.len() && inner.is_none() {
          continue;
        }
        let k1 = inner.map_or(upper.len().saturating_sub(1), |u| self.pos[u]);
        for &w in &lower[scanned..=l1] {
          for &u in &self.up[w] {
            let k = self.pos[u];
            if (k < k0 || k > k1) && !self.inner(u, w) {
              marked.insert((u, w));
            }
          }
        }
        scanned = l1 + 1;
        k0 = k1;
      }
    }
    marked
  }

  /// A slot's index within its row, counted from the side `dir` packs to.
  fn order(&self, v: usize, dir: Direction) -> usize {
    if dir.left {
      self.pos[v]
    } else {
      self.rows[self.layer[v]].len() - 1 - self.pos[v]
    }
  }

  /// Vertical alignment: join each slot to a median neighbour in the row it
  /// is aligned against, unless that would cross an alignment already made
  /// (or a long edge). Returns each slot's block, named by its first slot.
  fn align(
    &self,
    conflicts: &HashSet<(usize, usize)>,
    dir: Direction,
  ) -> Vec<usize> {
    let n = self.slots.len();
    let mut root: Vec<usize> = (0..n).collect();
    let mut align: Vec<usize> = (0..n).collect();
    let layers: Vec<&Vec<usize>> = if dir.down {
      self.rows.iter().collect()
    } else {
      self.rows.iter().rev().collect()
    };
    for row in layers.into_iter().skip(1) {
      let mut row: Vec<usize> = row.clone();
      if !dir.left {
        row.reverse();
      }
      // The furthest neighbour aligned with so far in this row, so later
      // alignments cannot cross it.
      let mut reached: Option<usize> = None;
      for v in row {
        let mut nbrs =
          if dir.down { &self.up[v] } else { &self.down[v] }.clone();
        if nbrs.is_empty() {
          continue;
        }
        nbrs.sort_by_key(|&u| self.order(u, dir));
        let d = nbrs.len();
        let medians = if d % 2 == 1 {
          vec![d / 2]
        } else {
          vec![d / 2 - 1, d / 2]
        };
        for m in medians {
          if align[v] != v {
            break;
          }
          let u = nbrs[m];
          let segment = if dir.down { (u, v) } else { (v, u) };
          let at = self.order(u, dir);
          if !conflicts.contains(&segment) && reached.is_none_or(|r| r < at) {
            align[u] = v;
            root[v] = root[u];
            align[v] = root[v];
            reached = Some(at);
          }
        }
      }
    }
    root
  }

  /// Horizontal compaction: every block as far towards `dir`'s side as the
  /// gaps allow, then pulled back towards its neighbour on the other side
  /// where it has room, so no block hangs out on its own. Returns every
  /// slot's x.
  fn compact(&self, root: &[usize], dir: Direction, gap: f64) -> Vec<f64> {
    let n = self.slots.len();
    // Block graph: an edge from each block to the next block along a row,
    // weighted with the centre-to-centre separation the pair needs.
    let mut next: HashMap<(usize, usize), f64> = HashMap::new();
    for row in &self.rows {
      let mut row = row.clone();
      if !dir.left {
        row.reverse();
      }
      for pair in row.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let sep = (self.width[a] + self.width[b]) / 2.0 + gap;
        let key = (root[a], root[b]);
        let e = next.entry(key).or_insert(sep);
        *e = e.max(sep);
      }
    }
    let mut succ: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut pred: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut edges: Vec<_> = next.into_iter().collect();
    edges.sort_by_key(|&((a, b), _)| (a, b));
    for ((a, b), sep) in edges {
      succ[a].push((b, sep));
      pred[b].push((a, sep));
    }

    // Blocks in topological order (Kahn's, lowest number first).
    let blocks: Vec<usize> = (0..n).filter(|&v| root[v] == v).collect();
    let mut indegree: Vec<usize> = pred.iter().map(Vec::len).collect();
    let mut ready: Vec<usize> = blocks
      .iter()
      .copied()
      .filter(|&b| indegree[b] == 0)
      .rev()
      .collect();
    let mut topo = Vec::with_capacity(blocks.len());
    while let Some(b) = ready.pop() {
      topo.push(b);
      for &(s, _) in &succ[b] {
        indegree[s] -= 1;
        if indegree[s] == 0 {
          ready.push(s);
        }
      }
    }

    // Pass one: the smallest coordinate each block can take.
    let mut x = vec![0.0; n];
    for &b in &topo {
      x[b] = pred[b]
        .iter()
        .map(|&(a, sep)| x[a] + sep)
        .fold(0.0, f64::max);
    }
    // Pass two: the greatest, short of the next block along.
    for &b in topo.iter().rev() {
      let limit = succ[b]
        .iter()
        .map(|&(s, sep)| x[s] - sep)
        .fold(f64::INFINITY, f64::min);
      if limit.is_finite() {
        x[b] = x[b].max(limit);
      }
    }

    let sign = if dir.left { 1.0 } else { -1.0 };
    (0..n).map(|v| sign * x[root[v]]).collect()
  }

  /// The left and right edges of everything in `xs`.
  fn extent(&self, xs: &[f64]) -> (f64, f64) {
    xs.iter()
      .zip(&self.width)
      .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (x, w)| {
        (lo.min(x - w / 2.0), hi.max(x + w / 2.0))
      })
  }
}
