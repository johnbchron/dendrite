//! The canvas scene: what the canvas draws, built from the graph under the
//! active lens.

use std::{collections::HashSet, sync::Arc};

use base::{Graph, NodeId, NodeState, QuestId};
use layout::{Layout, Slot};
use session::Session;

use super::AppState;
use crate::{
  formula::describe,
  scene::{CanvasScene, Category, Membership, RenderEdge, RenderNode},
};

/// Which nodes a lens shows, and which of those it dims.
struct Lens {
  /// Every node drawn.
  visible: HashSet<NodeId>,
  /// The nodes the quest claims; the rest of `visible` is only pulled in.
  claimed: HashSet<NodeId>,
  /// Whether a quest lens is on (else every node shows, none dimmed).
  scoped:  bool,
}

impl Lens {
  /// The lens of `quest` over `graph`, or the global view for `None`.
  fn new(graph: &Graph, quest: Option<QuestId>) -> Self {
    match quest {
      Some(q) => {
        let s = base::scope(graph, q);
        Self {
          visible: s.all().collect(),
          claimed: s.claimed.clone(),
          scoped:  true,
        }
      }
      None => Self {
        visible: graph.nodes().map(|n| n.id).collect(),
        claimed: HashSet::new(),
        scoped:  false,
      },
    }
  }

  /// Whether `node` is drawn.
  fn shows(&self, node: NodeId) -> bool { self.visible.contains(&node) }

  /// Whether `node` is only pulled into the quest's scope, not claimed.
  fn dims(&self, node: NodeId) -> bool {
    self.scoped && !self.claimed.contains(&node)
  }
}

impl AppState {
  /// The paint scene for the canvas, honouring the active quest lens.
  ///
  /// Rebuilt only when the graph, the selection or the lens changed; other
  /// calls return the same `Arc`, which the canvas takes as "nothing new".
  pub fn scene(&self) -> Arc<CanvasScene> {
    let store = self.lock();
    let key = (
      store.revision(),
      self.facts_revision,
      self.selected,
      self.active_quest,
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
    let lay: &Layout = match self.active_quest {
      Some(quest) => {
        lens_layout = self.caches.lens_layout(store, quest);
        &lens_layout
      }
      None => &cached.layout,
    };
    let lens = Lens::new(graph, self.active_quest);
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
          dimmed:   lens.dims(node.id),
          quest:    membership(node),
        })
      })
      .collect();

    let edges = graph
      .edges()
      .filter(|edge| lens.shows(edge.from) && lens.shows(edge.to))
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
