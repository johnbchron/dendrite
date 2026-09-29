//! The canvas scene: what the canvas draws, built from the graph under the
//! active lens.

use std::{collections::HashSet, sync::Arc};

use base::{Completed, Graph, NodeId, NodeState};
use layout::{Layout, Slot};
use session::Session;

use super::{AppState, cache::Lens};
use crate::{
  formula::describe,
  scene::{CanvasScene, Category, Membership, RenderEdge, RenderNode},
};

/// Which nodes a view shows, and which of those it dims.
struct View {
  /// Every node drawn.
  visible: HashSet<NodeId>,
  /// The nodes shown at full strength; the rest of `visible` is dimmed.
  /// `None` when nothing is.
  full:    Option<HashSet<NodeId>>,
}

impl View {
  /// What `lens` shows of `graph`, or the main view for `None`: every node
  /// but the retired ones. A quest's lens leaves them out too, and dims
  /// what it only pulls in.
  fn new(graph: &Graph, completed: &Completed, lens: Option<Lens>) -> Self {
    let live = |n: &NodeId| !completed.retired.contains(n);
    match lens {
      Some(Lens::Quest(q)) => {
        let s = base::scope(graph, q);
        Self {
          visible: s.all().filter(live).collect(),
          full:    Some(s.claimed),
        }
      }
      Some(Lens::Completed) => Self {
        visible: completed.trees.clone(),
        full:    None,
      },
      None => Self {
        visible: graph.nodes().map(|n| n.id).filter(live).collect(),
        full:    None,
      },
    }
  }

  /// Whether `node` is drawn.
  fn shows(&self, node: NodeId) -> bool { self.visible.contains(&node) }

  /// Whether `node` is only pulled into the quest's scope, not claimed.
  fn dims(&self, node: NodeId) -> bool {
    self.full.as_ref().is_some_and(|full| !full.contains(&node))
  }
}

impl AppState {
  /// The paint scene for the canvas, honouring the lens in use.
  ///
  /// Rebuilt only when the graph, the selection or the lens changed; other
  /// calls return the same `Arc`, which the canvas takes as "nothing new".
  pub fn scene(&self) -> Arc<CanvasScene> {
    let store = self.lock();
    let key = (
      store.revision(),
      self.facts_revision,
      self.selected,
      self.lens(),
    );
    self.caches.scene(key, || self.build_scene(&store))
  }

  /// Build the paint scene from scratch.
  fn build_scene(&self, store: &Session) -> CanvasScene {
    let graph = store.graph();
    let cached = self.derivations(store);
    let derived = &cached.derived;
    // A lens is laid out as a graph of its own, so its nodes take the rows
    // their places in the quest call for, not the ones they hold among
    // every node.
    let lens_layout;
    let lay: &Layout = match self.lens() {
      Some(lens) => {
        lens_layout = self.caches.lens_layout(store, &cached.completed, lens);
        &lens_layout
      }
      None => &cached.layout,
    };
    let view = View::new(graph, &cached.completed, self.lens());
    let quests = base::all_quests_scope(graph);
    // A formula condition is shared too widely for a bar to say anything
    // about it.
    let membership = |node: &base::Node| {
      if !node.kind.claimable() {
        Membership::None
      } else if quests.claimed.contains(&node.id) {
        Membership::Direct
      } else if quests.pulled_in.contains(&node.id) {
        Membership::Indirect
      } else {
        Membership::None
      }
    };
    let today = self.today();

    // One box per drawn node: every node in view, and every extra copy of
    // a shared condition. Each copy shows its node's state.
    let nodes = lay
      .arrangement
      .rows
      .iter()
      .flatten()
      .filter_map(|slot| match *slot {
        Slot::Node(drawn) => Some(drawn),
        Slot::Bend { .. } => None,
      })
      .filter_map(|drawn| {
        let node = graph.node(lay.copies.node(drawn))?;
        Some(RenderNode {
          id:       drawn,
          node:     node.id,
          copies:   lay.copies.count(node.id),
          label:    describe::node_name(graph, node, today),
          kind:     node.kind.clone(),
          glyph:    node.kind.atom().map(Category::of),
          state:    derived.state(node.id).unwrap_or(NodeState::Blocked),
          selected: self.selected == Some(node.id),
          dimmed:   view.dims(node.id),
          quest:    membership(node),
        })
      })
      .collect();

    let edges = graph
      .edges()
      .filter(|edge| view.shows(edge.from) && view.shows(edge.to))
      .map(|edge| RenderEdge {
        id:       edge.id,
        from:     edge.from,
        to:       lay.copies.end(edge),
        reversed: lay.is_reversed(edge.id),
        to_copy:  lay.copies.count(edge.to) > 1,
      })
      .collect();

    let arrangement = lay.arrangement.clone();
    CanvasScene {
      nodes,
      edges,
      arrangement,
    }
  }
}
