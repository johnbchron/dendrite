//! The event log's payloads, the reducer, and inverse generation for undo
//! (PLAN §3).
//!
//! Every UI mutation is one [`Event`]. Events are the source of truth; the
//! [`Graph`] is a projection produced by folding [`Event::apply`] over the
//! log. Undo never deletes history — it appends the *inverse* event, so the
//! log stays monotonic and doubles as an audit trail.

use serde::{Deserialize, Serialize};

use crate::{
  graph::Graph,
  ids::{EdgeId, NodeId, QuestId},
  model::{Edge, EdgeKind, Node, NodeKind, Quest},
};

/// A single, self-describing mutation of the graph.
///
/// Serde tags every variant with a `"type"` field so a stored payload is
/// self-describing (PLAN §3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
  /// Add a node to the global namespace.
  NodeAdded {
    /// New node id.
    node:       NodeId,
    /// Task-or-condition payload (carries the initial done bit).
    kind:       NodeKind,
    /// Display name.
    name:       String,
    /// Initial within-level ordering hint.
    order_hint: f64,
  },
  /// Remove a node together with its incident edges and quest claims.
  NodeRemoved {
    /// Node to remove.
    node: NodeId,
  },
  /// Change a node's display name.
  NodeRenamed {
    /// Target node.
    node: NodeId,
    /// New name.
    name: String,
  },
  /// Change a node's within-level ordering hint.
  OrderHintChanged {
    /// Target node.
    node:       NodeId,
    /// New hint.
    order_hint: f64,
  },
  /// Set a task's completion bit.
  TaskCompleted {
    /// Target task node.
    node:      NodeId,
    /// Whether it is now completed.
    completed: bool,
  },
  /// Set a condition's satisfaction bit.
  ConditionSet {
    /// Target condition node.
    node:      NodeId,
    /// Whether it is now satisfied.
    satisfied: bool,
  },
  /// Add a first-class edge (`from` requires `to`).
  EdgeAdded {
    /// New edge id.
    edge: EdgeId,
    /// Dependency vs. subtask.
    kind: EdgeKind,
    /// Dependent / parent.
    from: NodeId,
    /// Requirement / child.
    to:   NodeId,
  },
  /// Remove an edge.
  EdgeRemoved {
    /// Edge to remove.
    edge: EdgeId,
  },
  /// Create a quest.
  QuestCreated {
    /// New quest id.
    quest: QuestId,
    /// Display name.
    name:  String,
  },
  /// Remove a quest (its claimed nodes are untouched).
  QuestRemoved {
    /// Quest to remove.
    quest: QuestId,
  },
  /// Claim a node for a quest.
  QuestClaimed {
    /// Quest gaining the claim.
    quest: QuestId,
    /// Node being claimed.
    node:  NodeId,
  },
  /// Release a quest's claim on a node.
  QuestUnclaimed {
    /// Quest losing the claim.
    quest: QuestId,
    /// Node being released.
    node:  NodeId,
  },
}

impl Event {
  /// Fold this event into `graph`, mutating it in place.
  ///
  /// Application is total and best-effort: an event that targets a missing
  /// entity is a no-op rather than an error, which keeps log replay robust.
  pub fn apply(&self, graph: &mut Graph) {
    match self {
      Event::NodeAdded {
        node,
        kind,
        name,
        order_hint,
      } => {
        graph.insert_node(Node::new(
          *node,
          name.clone(),
          kind.clone(),
          *order_hint,
        ));
      }
      Event::NodeRemoved { node } => {
        graph.remove_node(*node);
      }
      Event::NodeRenamed { node, name } => {
        graph.rename_node(*node, name.clone());
      }
      Event::OrderHintChanged { node, order_hint } => {
        graph.set_order_hint(*node, *order_hint);
      }
      Event::TaskCompleted { node, completed } => {
        graph.set_satisfied(*node, *completed);
      }
      Event::ConditionSet { node, satisfied } => {
        graph.set_satisfied(*node, *satisfied);
      }
      Event::EdgeAdded {
        edge,
        kind,
        from,
        to,
      } => {
        graph.insert_edge(Edge::new(*edge, *kind, *from, *to));
      }
      Event::EdgeRemoved { edge } => {
        graph.remove_edge(*edge);
      }
      Event::QuestCreated { quest, name } => {
        graph.insert_quest(Quest::new(*quest, name.clone()));
      }
      Event::QuestRemoved { quest } => {
        graph.remove_quest(*quest);
      }
      Event::QuestClaimed { quest, node } => {
        graph.claim(*quest, *node);
      }
      Event::QuestUnclaimed { quest, node } => {
        graph.unclaim(*quest, *node);
      }
    }
  }

  /// Produce the event(s) that undo this one, computed against the
  /// *pre-state* `graph` (the graph as it was *before* `self` was applied).
  ///
  /// A removal expands into a compound inverse that restores the node/quest
  /// plus every incident edge and claim, so undo is complete. Returns an
  /// empty vec when there is nothing to invert (e.g. a no-op event).
  pub fn inverse(&self, graph: &Graph) -> Vec<Event> {
    match self {
      Event::NodeAdded { node, .. } => {
        vec![Event::NodeRemoved { node: *node }]
      }
      Event::NodeRemoved { node } => {
        let Some(n) = graph.node(*node) else {
          return vec![];
        };
        let mut inv = vec![Event::NodeAdded {
          node:       n.id,
          kind:       n.kind.clone(),
          name:       n.name.clone(),
          order_hint: n.order_hint,
        }];
        // Restore incident edges (both directions), de-duplicated by id.
        let mut seen = std::collections::HashSet::new();
        for edge in graph
          .requirements_of(*node)
          .chain(graph.dependents_of(*node))
        {
          if seen.insert(edge.id) {
            inv.push(Event::EdgeAdded {
              edge: edge.id,
              kind: edge.kind,
              from: edge.from,
              to:   edge.to,
            });
          }
        }
        // Restore quest claims.
        for quest in graph.quests() {
          if quest.claims.contains(node) {
            inv.push(Event::QuestClaimed {
              quest: quest.id,
              node:  *node,
            });
          }
        }
        inv
      }
      Event::NodeRenamed { node, .. } => graph
        .node(*node)
        .map(|n| Event::NodeRenamed {
          node: *node,
          name: n.name.clone(),
        })
        .into_iter()
        .collect(),
      Event::OrderHintChanged { node, .. } => graph
        .node(*node)
        .map(|n| Event::OrderHintChanged {
          node:       *node,
          order_hint: n.order_hint,
        })
        .into_iter()
        .collect(),
      Event::TaskCompleted { node, .. } => graph
        .node(*node)
        .map(|n| Event::TaskCompleted {
          node:      *node,
          completed: n.kind.is_satisfied(),
        })
        .into_iter()
        .collect(),
      Event::ConditionSet { node, .. } => graph
        .node(*node)
        .map(|n| Event::ConditionSet {
          node:      *node,
          satisfied: n.kind.is_satisfied(),
        })
        .into_iter()
        .collect(),
      Event::EdgeAdded { edge, .. } => {
        vec![Event::EdgeRemoved { edge: *edge }]
      }
      Event::EdgeRemoved { edge } => graph
        .edge(*edge)
        .map(|e| Event::EdgeAdded {
          edge: e.id,
          kind: e.kind,
          from: e.from,
          to:   e.to,
        })
        .into_iter()
        .collect(),
      Event::QuestCreated { quest, .. } => {
        vec![Event::QuestRemoved { quest: *quest }]
      }
      Event::QuestRemoved { quest } => {
        let Some(q) = graph.quest(*quest) else {
          return vec![];
        };
        let mut inv = vec![Event::QuestCreated {
          quest: q.id,
          name:  q.name.clone(),
        }];
        for node in &q.claims {
          inv.push(Event::QuestClaimed {
            quest: *quest,
            node:  *node,
          });
        }
        inv
      }
      Event::QuestClaimed { quest, node } => {
        vec![Event::QuestUnclaimed {
          quest: *quest,
          node:  *node,
        }]
      }
      Event::QuestUnclaimed { quest, node } => {
        vec![Event::QuestClaimed {
          quest: *quest,
          node:  *node,
        }]
      }
    }
  }
}

/// Apply a batch of events in order, returning the batch that undoes it.
///
/// The returned inverse is ordered so that replaying it exactly reverses the
/// batch: each event's inverse is captured against the state *before* that
/// event, then the inverses are reversed as a whole.
pub fn apply_batch(graph: &mut Graph, events: &[Event]) -> Vec<Event> {
  let mut inverse = Vec::new();
  for event in events {
    let mut inv = event.inverse(graph);
    event.apply(graph);
    // Prepend (reverse order) so undo unwinds last-applied first.
    inv.reverse();
    inverse.extend(inv);
  }
  inverse.reverse();
  inverse
}
