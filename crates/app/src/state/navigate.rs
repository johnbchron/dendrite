//! Moving the selection with the arrow keys, as the canvas draws the graph.

use std::collections::HashMap;

use base::NodeId;
use layout::{Arrangement, Slot};

use super::AppState;
use crate::{camera::CameraRequest, keymap::Direction};

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
  ///
  /// Steps are taken between boxes, so a node drawn more than once moves
  /// from the copy the selection was reached through, and a step down
  /// lands on the copy of a shared condition that serves this node.
  pub fn navigate(&mut self, direction: Direction) {
    let Some(selected) = self.selected else {
      return;
    };
    let scene = self.scene();
    let grid = Grid::new(&scene.arrangement);
    let from = self
      .selected_copy
      .filter(|c| grid.at.contains_key(c))
      .unwrap_or(selected);
    if !grid.at.contains_key(&from) {
      return;
    }
    let target = match direction {
      Direction::Left => grid.beside(from, false),
      Direction::Right => grid.beside(from, true),
      Direction::Up | Direction::Down => {
        let up = direction == Direction::Up;
        let candidates = scene
          .edges
          .iter()
          .filter_map(|e| {
            if up {
              (e.to == from).then_some(e.from)
            } else {
              (e.from == from).then_some(e.to)
            }
          })
          .collect();
        grid.nearest(from, candidates)
      }
    };
    let Some(copy) = target else { return };
    let Some(node) = scene.nodes.iter().find(|n| n.id == copy) else {
      return;
    };
    self.select(Some(node.node));
    self.selected_copy = Some(copy);
    self.aim(CameraRequest::Reveal(copy));
  }
}
