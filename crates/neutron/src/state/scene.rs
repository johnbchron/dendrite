//! The canvas scene: what the canvas draws, built from the graph under the
//! active lens.

use std::{collections::HashSet, sync::Arc};

use base::{Graph, NodeId, NodeState, QuestId};
use db::Store;
use layout::Slot;

use super::AppState;
use crate::canvas::{CanvasScene, RenderEdge, RenderNode};

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
    let key = (store.revision(), self.selected, self.active_quest);
    self.caches.scene(key, || self.build_scene(&store))
  }

  /// Build the paint scene from scratch.
  fn build_scene(&self, store: &Store) -> CanvasScene {
    let graph = store.graph();
    let cached = self.derivations(store);
    let (derived, lay) = (&cached.derived, &cached.layout);
    let lens = Lens::new(graph, self.active_quest);

    let nodes = graph
      .nodes()
      .filter(|node| lens.shows(node.id))
      .map(|node| RenderNode {
        id:       node.id,
        label:    node.name.clone(),
        kind:     node.kind.clone(),
        state:    derived.state(node.id).unwrap_or(NodeState::Blocked),
        selected: self.selected == Some(node.id),
        dimmed:   lens.dims(node.id),
      })
      .collect();

    let edges = graph
      .edges()
      .filter(|edge| lens.shows(edge.from) && lens.shows(edge.to))
      .map(|edge| RenderEdge {
        id:       edge.id,
        from:     edge.from,
        to:       edge.to,
        reversed: lay.is_reversed(edge.id),
      })
      .collect();

    // Out-of-scope nodes give up their slots, so a lens shows its nodes
    // packed together rather than scattered among gaps.
    // A long edge keeps its channels only while both its ends are shown.
    let arrangement = lay.arrangement.retain(|slot| match slot {
      Slot::Node(n) => lens.shows(n),
      Slot::Bend { edge, .. } => graph
        .edge(edge)
        .is_some_and(|e| lens.shows(e.from) && lens.shows(e.to)),
    });
    CanvasScene {
      nodes,
      edges,
      arrangement,
    }
  }
}
