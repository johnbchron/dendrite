//! Moving the selection with the arrow keys, as the canvas draws the graph.

use std::collections::HashMap;

use base::NodeId;
use layout::{Arrangement, Slot};

use super::AppState;
use crate::keymap::Direction;

/// The drawn nodes by row and column, bends left out.
struct Grid {
  /// Row and column of every drawn node.
  at:   HashMap<NodeId, (usize, usize)>,
  /// Each row's nodes, left to right.
  rows: Vec<Vec<NodeId>>,
}

impl Grid {
  /// The nodes of `arrangement`, as drawn.
  fn new(arrangement: &Arrangement) -> Self {
    let mut at = HashMap::new();
    let mut rows = Vec::new();
    for (r, row) in arrangement.rows.iter().enumerate() {
      let nodes: Vec<NodeId> = row
        .iter()
        .filter_map(|slot| match slot {
          Slot::Node(n) => Some(*n),
          Slot::Bend { .. } => None,
        })
        .collect();
      for (c, n) in nodes.iter().enumerate() {
        at.insert(*n, (r, c));
      }
      rows.push(nodes);
    }
    Self { at, rows }
  }

  /// The node beside `from` in its row, towards `right` or left.
  fn beside(&self, from: NodeId, right: bool) -> Option<NodeId> {
    let &(row, col) = self.at.get(&from)?;
    if right {
      self.rows[row].get(col + 1).copied()
    } else {
      col.checked_sub(1).map(|c| self.rows[row][c])
    }
  }

  /// The drawn node among `candidates` nearest `from`: fewest rows away,
  /// then fewest columns, then lowest id.
  fn nearest(&self, from: NodeId, candidates: Vec<NodeId>) -> Option<NodeId> {
    let &(row, col) = self.at.get(&from)?;
    candidates
      .into_iter()
      .filter(|n| self.at.contains_key(n))
      .min_by_key(|n| {
        let (r, c) = self.at[n];
        (r.abs_diff(row), c.abs_diff(col), *n)
      })
  }
}

impl AppState {
  /// Move the selection to a neighbour, as the canvas draws the graph: up
  /// to a node that requires it, down to one it requires (the nearest in
  /// its row, if several), or left and right along its row. Stays put at an
  /// edge.
  pub fn navigate(&mut self, direction: Direction) {
    let Some(from) = self.selected else { return };
    let grid = Grid::new(&self.scene().arrangement);
    if !grid.at.contains_key(&from) {
      return;
    }
    let target = match direction {
      Direction::Left => grid.beside(from, false),
      Direction::Right => grid.beside(from, true),
      Direction::Up | Direction::Down => {
        let store = self.lock();
        let graph = store.graph();
        let candidates = if direction == Direction::Up {
          graph.dependents_of(from).map(|e| e.from).collect()
        } else {
          graph.requirements_of(from).map(|e| e.to).collect()
        };
        drop(store);
        grid.nearest(from, candidates)
      }
    };
    if let Some(target) = target {
      self.go_to(target);
    }
  }
}
